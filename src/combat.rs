use crate::cards::{Card, CardId, Eff, Kind};
use crate::enemies::{Enemy, EnemyKind, Intent, pick_intent, spawn};
use crate::model::{Class, Fighter, Relic, St};
use crate::rng::Rng;
use crate::run::RunState;

pub const MAX_HAND: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Who {
    Player,
    Enemy(usize),
}

/// Things that happened in combat, consumed by the UI to spawn effects.
#[derive(Clone, Debug)]
pub enum Ev {
    Damage(Who, i32, i32),
    Dodge(Who),
    Block(Who, i32),
    Heal(Who, i32),
    Status(Who, St, i32),
    Died(Who),
    Played(Card),
    Hit(Who, Who),
    Banner(String),
    Text(Who, String),
}

pub struct Combat {
    pub class: Class,
    pub relics: Vec<Relic>,
    pub p: Fighter,
    pub energy: i32,
    pub max_energy: i32,
    pub draw_n: i32,
    pub draw: Vec<Card>,
    pub hand: Vec<Card>,
    pub discard: Vec<Card>,
    pub exhausted: Vec<Card>,
    pub enemies: Vec<Enemy>,
    pub floor: usize,
    pub turn: u32,
    pub played: u32,
    pub events: Vec<Ev>,
    pub log: Vec<String>,
    pub kills: u32,
    rng: Rng,
}

impl Combat {
    pub fn new(run: &RunState, kinds: &[EnemyKind], seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let info = run.class.info();
        let enemies = kinds
            .iter()
            .map(|&k| spawn(k, run.floor, &mut rng))
            .collect();
        let mut draw = run.deck.clone();
        rng.shuffle(&mut draw);
        Combat {
            class: run.class,
            relics: run.relics.clone(),
            p: Fighter {
                hp: run.hp,
                max_hp: run.max_hp,
                block: 0,
                st: Default::default(),
            },
            energy: 0,
            max_energy: info.energy,
            draw_n: info.draw,
            draw,
            hand: Vec::new(),
            discard: Vec::new(),
            exhausted: Vec::new(),
            enemies,
            floor: run.floor,
            turn: 0,
            played: 0,
            events: Vec::new(),
            log: Vec::new(),
            kills: 0,
            rng,
        }
    }

    pub fn has(&self, r: Relic) -> bool {
        self.relics.contains(&r)
    }

    fn log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }

    pub fn start(&mut self) {
        if self.has(Relic::RustedNail) {
            self.p.st.add(St::Strength, 1);
        }
        if self.has(Relic::ThornedRosary) {
            self.p.st.add(St::Thorns, 3);
        }
        if self.class == Class::Assassin {
            self.p.st.add(St::Dodge, 1);
        }
        let names: Vec<&str> = self.enemies.iter().map(|e| e.kind.name()).collect();
        let msg = format!("From the dark emerge: {}.", names.join(", "));
        self.log(msg);
        self.start_player_turn();
        if self.has(Relic::MournersVeil) {
            self.gain_block(10);
        }
        if self.has(Relic::GraveDust) {
            self.apply_all(St::Weak, 1);
        }
    }

    pub fn f(&mut self, w: Who) -> &mut Fighter {
        match w {
            Who::Player => &mut self.p,
            Who::Enemy(i) => &mut self.enemies[i].f,
        }
    }

    pub fn fr(&self, w: Who) -> &Fighter {
        match w {
            Who::Player => &self.p,
            Who::Enemy(i) => &self.enemies[i].f,
        }
    }

    pub fn alive(&self, w: Who) -> bool {
        match w {
            Who::Player => self.p.hp > 0,
            Who::Enemy(i) => !self.enemies[i].dead,
        }
    }

    pub fn won(&self) -> bool {
        self.enemies.iter().all(|e| e.dead)
    }

    pub fn lost(&self) -> bool {
        self.p.hp <= 0
    }

    pub fn first_alive(&self) -> Option<usize> {
        self.enemies.iter().position(|e| !e.dead)
    }

    fn name(&self, w: Who) -> String {
        match w {
            Who::Player => "You".into(),
            Who::Enemy(i) => self.enemies[i].kind.name().into(),
        }
    }

    /// Damage of one attack hit from `from` to `to` after modifiers.
    pub fn attack_value(&self, from: Who, to: Who, base: i32) -> i32 {
        let a = self.fr(from);
        let mut dmg = (base + a.st.get(St::Strength)).max(0);
        if a.st.get(St::Weak) > 0 {
            dmg = dmg * 3 / 4;
        }
        if self.fr(to).st.get(St::Vulnerable) > 0 {
            dmg = dmg * 3 / 2;
        }
        dmg
    }

    /// Resolves one hit; returns HP actually lost by the defender.
    fn attack(&mut self, from: Who, to: Who, base: i32) -> i32 {
        if !self.alive(to) || !self.alive(from) {
            return 0;
        }
        let dmg = self.attack_value(from, to, base);
        self.events.push(Ev::Hit(from, to));
        if self.fr(to).st.get(St::Dodge) > 0 {
            self.f(to).st.add(St::Dodge, -1);
            self.events.push(Ev::Dodge(to));
            return 0;
        }
        let lost = self.raw_damage(to, dmg);
        let thorns = self.fr(to).st.get(St::Thorns);
        if thorns > 0 {
            self.lose_hp(from, thorns, false);
        }
        if from == Who::Player {
            let venom = self.p.st.get(St::VenomBlades);
            if venom > 0 && self.alive(to) {
                self.apply(to, St::Poison, venom);
            }
        }
        self.check_deaths();
        lost
    }

    /// Damage that is reduced by block.
    fn raw_damage(&mut self, to: Who, dmg: i32) -> i32 {
        let t = self.f(to);
        let blocked = dmg.min(t.block);
        t.block -= blocked;
        let lost = dmg - blocked;
        t.hp -= lost;
        self.events.push(Ev::Damage(to, lost, blocked));
        lost
    }

    /// HP loss that ignores block.
    fn lose_hp(&mut self, w: Who, n: i32, self_inflicted: bool) {
        if n <= 0 || !self.alive(w) {
            return;
        }
        self.f(w).hp -= n;
        self.events.push(Ev::Damage(w, n, 0));
        if self_inflicted && w == Who::Player && self.class == Class::BloodKnight && self.p.hp > 0 {
            self.gain_block(n * 2);
        }
        self.check_deaths();
    }

    fn gain_block(&mut self, n: i32) {
        self.add_block(Who::Player, n);
    }

    fn add_block(&mut self, w: Who, n: i32) {
        let f = self.f(w);
        f.block = f.block.saturating_add(n);
        self.events.push(Ev::Block(w, n));
    }

    /// Applies a status to every enemy (dead ones are skipped by `apply`).
    fn apply_all(&mut self, s: St, n: i32) {
        for i in 0..self.enemies.len() {
            self.apply(Who::Enemy(i), s, n);
        }
    }

    fn heal(&mut self, w: Who, n: i32) {
        let f = self.f(w);
        let before = f.hp;
        f.hp = (f.hp + n).min(f.max_hp);
        let healed = f.hp - before;
        if healed > 0 {
            self.events.push(Ev::Heal(w, healed));
        }
    }

    fn apply(&mut self, w: Who, s: St, n: i32) {
        if !self.alive(w) || n == 0 {
            return;
        }
        self.f(w).st.add(s, n);
        self.events.push(Ev::Status(w, s, n));
    }

    fn check_deaths(&mut self) {
        for i in 0..self.enemies.len() {
            let e = &mut self.enemies[i];
            if !e.dead && e.f.hp <= 0 {
                e.dead = true;
                e.f.hp = 0;
                e.f.block = 0;
                self.kills += 1;
                self.events.push(Ev::Died(Who::Enemy(i)));
                let name = self.enemies[i].kind.name();
                self.log(format!("{name} is slain."));
                if self.class == Class::Warlock && self.p.hp > 0 {
                    self.heal(Who::Player, 3);
                }
                if self.has(Relic::EyeOfTheAbyss) && !self.won() {
                    self.draw_cards(1);
                }
            } else if !e.dead
                && e.kind == EnemyKind::Morvath
                && e.phase == 0
                && e.f.hp * 2 <= e.f.max_hp
            {
                e.phase = 1;
                e.step = 0;
                e.f.st.add(St::Strength, 3);
                e.f.st.set(St::Weak, 0);
                e.f.st.set(St::Vulnerable, 0);
                pick_intent(e, self.floor, &mut self.rng);
                self.events.push(Ev::Banner("MORVATH ASCENDS".into()));
                self.events.push(Ev::Status(Who::Enemy(i), St::Strength, 3));
                self.log("Morvath tears open the veil and ascends! (+3 Strength)");
            }
        }
        if self.p.hp <= 0
            && !self
                .events
                .iter()
                .any(|e| matches!(e, Ev::Died(Who::Player)))
        {
            self.events.push(Ev::Died(Who::Player));
        }
    }

    pub fn draw_cards(&mut self, n: i32) {
        for _ in 0..n {
            if self.hand.len() >= MAX_HAND {
                break;
            }
            if self.draw.is_empty() {
                if self.discard.is_empty() {
                    break;
                }
                self.draw.append(&mut self.discard);
                self.rng.shuffle(&mut self.draw);
                self.events.push(Ev::Text(Who::Player, "Reshuffle".into()));
            }
            if let Some(c) = self.draw.pop() {
                self.hand.push(c);
            }
        }
    }

    pub fn start_player_turn(&mut self) {
        self.turn += 1;
        self.played = 0;
        self.p.block = 0;
        let poison = self.p.st.get(St::Poison);
        if poison > 0 {
            self.lose_hp(Who::Player, poison, false);
            self.p.st.tick_down(St::Poison);
            self.log(format!("Poison gnaws at you for {poison}."));
            if self.lost() {
                return;
            }
        }
        let aura = self.p.st.get(St::DoomAura);
        if aura > 0 {
            self.apply_all(St::Doom, aura);
        }
        self.energy = self.max_energy;
        let mut draw = self.draw_n;
        if self.turn == 1 {
            if self.has(Relic::EmberOfHate) {
                self.energy += 1;
            }
            if self.has(Relic::BlackCandle) {
                draw += 2;
            }
        }
        self.draw_cards(draw);
        self.events.push(Ev::Banner(format!("TURN {}", self.turn)));
    }

    /// Why a card can't be played right now, if it can't.
    pub fn unplayable_reason(&self, i: usize) -> Option<&'static str> {
        let d = self.hand.get(i)?.def();
        if d.kind == Kind::Curse {
            return Some("Curses cannot be played.");
        }
        if d.cost > self.energy {
            return Some("Not enough energy.");
        }
        let blood = d.self_damage();
        if blood > 0 && self.p.hp <= blood {
            return Some("Not enough blood left to pay.");
        }
        None
    }

    /// Plays the card at hand index `i`. Returns false if it could not be played.
    pub fn play(&mut self, i: usize, target: Option<usize>) -> bool {
        if i >= self.hand.len() || self.unplayable_reason(i).is_some() {
            return false;
        }
        let card = self.hand[i];
        let d = card.def();
        let tgt = match target {
            Some(t) if t < self.enemies.len() && !self.enemies[t].dead => t,
            _ => match self.first_alive() {
                Some(t) => t,
                None => return false,
            },
        };
        self.hand.remove(i);
        self.energy -= d.cost;
        let combo = self.played > 0;
        self.played += 1;
        self.events.push(Ev::Played(card));
        let msg = if d.needs_target() {
            format!(
                "You play {} on {}.",
                card.name(),
                self.enemies[tgt].kind.name()
            )
        } else {
            format!("You play {}.", card.name())
        };
        self.log(msg);
        for eff in &d.effs {
            if self.lost() {
                break;
            }
            self.resolve(*eff, tgt, combo);
        }
        match d.kind {
            Kind::Power => {}
            _ if d.exhaust => self.exhausted.push(card),
            _ => self.discard.push(card),
        }
        self.check_deaths();
        true
    }

    fn resolve(&mut self, eff: Eff, tgt: usize, combo: bool) {
        let t = Who::Enemy(tgt);
        let pl = Who::Player;
        match eff {
            Eff::Dmg(n) => {
                self.attack(pl, t, n);
            }
            Eff::DmgAll(n) => {
                for i in 0..self.enemies.len() {
                    self.attack(pl, Who::Enemy(i), n);
                }
            }
            Eff::Multi(n, k) => {
                for _ in 0..k {
                    self.attack(pl, t, n);
                }
            }
            Eff::Block(n) => self.gain_block(n),
            Eff::Draw(n) => self.draw_cards(n),
            Eff::Energy(n) => {
                self.energy += n;
                self.events.push(Ev::Text(pl, format!("+{n} Energy")));
            }
            Eff::LoseHp(n) => self.lose_hp(pl, n, true),
            Eff::Heal(n) => self.heal(pl, n),
            Eff::Apply(s, n) => self.apply(t, s, n),
            Eff::ApplyAll(s, n) => self.apply_all(s, n),
            Eff::Gain(s, n) => self.apply(pl, s, n),
            Eff::ComboDmg(n) => {
                if combo {
                    self.events.push(Ev::Text(pl, "Combo!".into()));
                    self.attack(pl, t, n);
                }
            }
            Eff::ComboDraw(n) => {
                if combo {
                    self.events.push(Ev::Text(pl, "Combo!".into()));
                    self.draw_cards(n);
                }
            }
            Eff::DmgFromBlock => {
                let b = self.p.block;
                self.attack(pl, t, b);
            }
            Eff::DmgFromDoom => {
                let d = self.enemies[tgt].f.st.get(St::Doom);
                self.attack(pl, t, d);
            }
            Eff::DoublePoison => {
                let p = self.enemies[tgt].f.st.get(St::Poison);
                self.apply(t, St::Poison, p);
            }
            Eff::Execute(n) => {
                let e = &self.enemies[tgt].f;
                let n = if e.hp * 2 < e.max_hp { n * 2 } else { n };
                self.attack(pl, t, n);
            }
            Eff::Drain(n) => {
                let lost = self.attack(pl, t, n);
                self.heal(pl, lost);
            }
        }
    }

    pub fn end_player_turn(&mut self) {
        let plating = self.p.st.get(St::Plating);
        if plating > 0 {
            self.gain_block(plating);
        }
        let mut hand = std::mem::take(&mut self.hand);
        self.discard.append(&mut hand);
        self.p.st.tick_down(St::Weak);
        self.p.st.tick_down(St::Vulnerable);
        for i in 0..self.enemies.len() {
            let e = &mut self.enemies[i];
            if !e.dead && e.f.st.get(St::Doom) >= e.f.hp {
                e.f.hp = 0;
                self.events.push(Ev::Text(Who::Enemy(i), "DOOMED".into()));
                let name = e.kind.name();
                self.log(format!("Doom claims the soul of {name}."));
            }
        }
        self.check_deaths();
    }

    /// One enemy takes its turn.
    pub fn enemy_act(&mut self, i: usize) {
        if self.enemies[i].dead || self.lost() {
            return;
        }
        let me = Who::Enemy(i);
        self.enemies[i].f.block = 0;
        let poison = self.enemies[i].f.st.get(St::Poison);
        if poison > 0 {
            self.lose_hp(me, poison, false);
            self.enemies[i].f.st.tick_down(St::Poison);
            if self.enemies[i].dead {
                return;
            }
        }
        let name = self.name(me);
        let pl = Who::Player;
        match self.enemies[i].intent {
            Intent::Attack(n) => {
                let lost = self.attack(me, pl, n);
                self.log(format!("{name} strikes you ({lost} damage)."));
            }
            Intent::Multi(n, k) => {
                for _ in 0..k {
                    self.attack(me, pl, n);
                }
                self.log(format!("{name} attacks {k} times."));
            }
            Intent::Defend(b) => {
                self.add_block(me, b);
                self.log(format!("{name} braces itself."));
            }
            Intent::AttackDefend(n, b) => {
                self.attack(me, pl, n);
                self.add_block(me, b);
                self.log(format!("{name} strikes and raises its guard."));
            }
            Intent::Buff(s) => {
                self.apply(me, St::Strength, s);
                self.log(format!("{name} chants a dark ritual."));
            }
            Intent::BuffDefend(s, b) => {
                self.apply(me, St::Strength, s);
                self.add_block(me, b);
                self.log(format!("{name} gathers unholy power."));
            }
            Intent::Debuff(s, n) => {
                self.apply(pl, s, n);
                self.log(format!("{name} afflicts you with {}.", s.name()));
            }
            Intent::AttackDebuff(n, s, k) => {
                self.attack(me, pl, n);
                self.apply(pl, s, k);
                self.log(format!("{name} strikes, inflicting {}.", s.name()));
            }
            Intent::Curse(n) => {
                for _ in 0..n {
                    self.discard.push(Card::new(CardId::Wound));
                }
                self.events.push(Ev::Text(pl, format!("+{n} Wound")));
                self.log(format!(
                    "{name} curses you: {n} Wound(s) added to your discard."
                ));
            }
        }
        if self.alive(me) {
            let e = &mut self.enemies[i];
            e.f.st.tick_down(St::Weak);
            e.f.st.tick_down(St::Vulnerable);
            e.step += 1;
            pick_intent(e, self.floor, &mut self.rng);
        }
        self.check_deaths();
    }
}
