//! A short scripted encounter that walks a new player through combat.

use crate::cards::{Card, CardId, Eff, Kind};
use crate::combat::Combat;
use crate::enemies::{EnemyKind, Intent};
use crate::model::Class;
use crate::run::RunState;

pub const CLASS: Class = Class::BloodKnight;

/// What the player must do to move past a step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Goal {
    /// Press Enter to continue.
    Read,
    PlayAttack,
    GainBlock,
    Play(CardId),
    EndTurn,
    /// Enemy turn in progress; advances when the player's next turn begins.
    Watch,
    /// Free play until every enemy is dead.
    Win,
    /// Victory reached; Enter returns to the title.
    Done,
}

pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub goal: Goal,
}

pub const STEPS: &[Step] = &[
    Step {
        title: "Welcome, Pilgrim",
        text: "Combat is a duel of cards. Slay every enemy before your HP (♥, top left) runs out. \
               This Ghoul is weak — a fine first victim.",
        goal: Goal::Read,
    },
    Step {
        title: "Read the Intent",
        text: "The red text above the Ghoul is its INTENT: what it will do on its turn. \
               \"» ATTACK 7\" means it will strike you for 7. Enemies always telegraph their moves.",
        goal: Goal::Read,
    },
    Step {
        title: "Energy & Cards",
        text: "Each turn you get 3 Energy (bottom right) and draw 5 cards. The gold number in a \
               card's corner is its Energy cost.",
        goal: Goal::Read,
    },
    Step {
        title: "Strike",
        text: "Select Cleave with ←→ and press Enter — or click it, or press its number — to hit the Ghoul.",
        goal: Goal::PlayAttack,
    },
    Step {
        title: "Raise Your Guard",
        text: "The Ghoul will hit you for 7. Play Iron Guard: Block (◆) absorbs incoming damage, \
               but it crumbles at the start of your next turn.",
        goal: Goal::GainBlock,
    },
    Step {
        title: "Blood for Power",
        text: "Blood Offering costs no Energy but 3 HP, and grants 2 Energy. As a Blood Knight, HP you \
               spend on your own cards also grants twice that in Block. Play it.",
        goal: Goal::Play(CardId::BloodOffering),
    },
    Step {
        title: "End Your Turn",
        text: "Spend your remaining Energy as you like, then press E (or click END TURN). \
               Energy you don't use is lost.",
        goal: Goal::EndTurn,
    },
    Step {
        title: "The Enemy Acts",
        text: "Watch the Ghoul carry out its intent. Your Block takes the blow first.",
        goal: Goal::Watch,
    },
    Step {
        title: "A New Turn",
        text: "Your unplayed cards were discarded; Energy refilled and 5 fresh cards were drawn. \
               When your draw pile runs dry, the discard pile is shuffled back in.",
        goal: Goal::Read,
    },
    Step {
        title: "Finish It",
        text: "Check the Ghoul's new intent and put it down for good. Press ? at any time for the \
               Codex, which explains every status and intent.",
        goal: Goal::Win,
    },
    Step {
        title: "Victory",
        text: "The Ghoul is dust. That is all you need to begin your descent — rewards, relics and \
               stranger foes await below. Press Enter to return to the title.",
        goal: Goal::Done,
    },
];

pub struct Tutorial {
    pub step: usize,
    pub t0: f32,
}

impl Tutorial {
    pub fn new(t: f32) -> Self {
        Tutorial { step: 0, t0: t }
    }

    pub fn current(&self) -> &'static Step {
        &STEPS[self.step.min(STEPS.len() - 1)]
    }

    pub fn goal(&self) -> Goal {
        self.current().goal
    }

    pub fn next(&mut self, t: f32) {
        if self.step + 1 < STEPS.len() {
            self.step += 1;
            self.t0 = t;
        }
    }

    /// Why the card can't be played at this step, if it can't.
    pub fn blocks_card(&self, card: Card) -> Option<&'static str> {
        let d = card.def();
        match self.goal() {
            Goal::Read => Some("Press Enter to continue."),
            Goal::PlayAttack if d.kind != Kind::Attack => Some("Play an Attack first — try Cleave."),
            Goal::GainBlock if !d.effs.iter().any(|e| matches!(e, Eff::Block(_))) => Some("Play a Block card — try Iron Guard."),
            Goal::Play(id) if card.id != id => Some("Play Blood Offering."),
            Goal::Watch | Goal::Done => Some(""),
            _ => None,
        }
    }

    pub fn allows_end_turn(&self) -> bool {
        matches!(self.goal(), Goal::EndTurn | Goal::Win)
    }

    /// Advances past an action step once the played card satisfies it.
    pub fn on_play(&mut self, card: Card, t: f32) {
        let d = card.def();
        let done = match self.goal() {
            Goal::PlayAttack => d.kind == Kind::Attack,
            Goal::GainBlock => d.effs.iter().any(|e| matches!(e, Eff::Block(_))),
            Goal::Play(id) => card.id == id,
            _ => false,
        };
        if done {
            self.next(t);
        }
    }
}

/// A throwaway run (never saved) for the tutorial's status bar and combat.
pub fn run() -> RunState {
    RunState::new(CLASS)
}

/// Builds the scripted encounter: a fixed opening hand and a predictable Ghoul.
pub fn combat(run: &RunState) -> Combat {
    use CardId::*;
    let mut c = Combat::new(run, &[EnemyKind::Ghoul], 7);
    // Cards are drawn from the end of the pile, so the opening hand is listed last, reversed.
    let order = [IronGuard, Cleave, CrimsonArc, BloodOffering, IronGuard, Cleave, IronGuard, Cleave];
    c.draw = order.iter().map(|&id| Card::new(id)).collect();
    let g = &mut c.enemies[0];
    g.f.hp = 22;
    g.f.max_hp = 22;
    g.step = 0;
    g.intent = Intent::Attack(7);
    c.start();
    c
}
