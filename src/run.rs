use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::cards::{Card, Rarity, class_pool, starter_deck};
use crate::model::{Class, Relic};
use crate::rng::Rng;
use crate::theme;

pub const FLOORS: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RoomKind {
    Combat,
    Elite,
    Shrine,
    Treasure,
    Merchant,
    Event,
    Boss,
}

impl RoomKind {
    pub fn name(self) -> &'static str {
        match self {
            RoomKind::Combat => "Combat",
            RoomKind::Elite => "Elite",
            RoomKind::Shrine => "Shrine",
            RoomKind::Treasure => "Relic",
            RoomKind::Merchant => "Merchant",
            RoomKind::Event => "Unknown",
            RoomKind::Boss => "BOSS",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            RoomKind::Combat => {
                "Restless dead stir behind this door. Fight for gold and a new card."
            }
            RoomKind::Elite => "A powerful horror waits within. Dangerous, but it guards a relic.",
            RoomKind::Shrine => "A ruined chapel. Rest to heal, or pray to empower a card.",
            RoomKind::Treasure => "A sealed sarcophagus. Something precious lies inside.",
            RoomKind::Merchant => "The Hollow Merchant trades in cards and relics. Bring gold.",
            RoomKind::Event => "Whispers and shadows. Anything could be waiting.",
            RoomKind::Boss => "Morvath, the Unholy Abbot, waits on his throne of bones.",
        }
    }

    pub fn color(self) -> Color {
        match self {
            RoomKind::Combat => theme::BLOOD_LIGHT,
            RoomKind::Elite => theme::EMBER,
            RoomKind::Shrine => theme::HEAL,
            RoomKind::Treasure => theme::GOLD,
            RoomKind::Merchant => theme::STEEL,
            RoomKind::Event => theme::VIOLET,
            RoomKind::Boss => theme::BLOOD,
        }
    }

    pub fn icon(self) -> [&'static str; 3] {
        match self {
            RoomKind::Combat => [r"\   /", r" \ / ", r" / \ "],
            RoomKind::Elite => [r" ___ ", r"(o o)", r" |^| "],
            RoomKind::Shrine => [r"  |  ", r" -+- ", r"  |  "],
            RoomKind::Treasure => [r" ___ ", r"[_$_]", r"[___]"],
            RoomKind::Merchant => [r" _)_ ", r"( $ )", r" `-' "],
            RoomKind::Event => [r" .-. ", r"  .' ", r"  .  "],
            RoomKind::Boss => [r"\___/", r"(@ @)", r" vvv "],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunState {
    pub class: Class,
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck: Vec<Card>,
    pub relics: Vec<Relic>,
    pub floor: usize,
    pub map: Vec<Vec<RoomKind>>,
    pub path: Vec<usize>,
    pub pending: Option<usize>,
    pub rng: Rng,
    pub kills: u32,
    pub elapsed: f64,
}

impl RunState {
    pub fn new(class: Class) -> Self {
        let mut rng = Rng::from_time();
        let info = class.info();
        let map = gen_map(&mut rng);
        RunState {
            class,
            hp: info.hp,
            max_hp: info.hp,
            gold: 40,
            deck: starter_deck(class),
            relics: Vec::new(),
            floor: 0,
            map,
            path: Vec::new(),
            pending: None,
            rng,
            kills: 0,
            elapsed: 0.0,
        }
    }

    pub fn current_room(&self) -> Option<RoomKind> {
        self.pending
            .and_then(|i| self.map.get(self.floor)?.get(i).copied())
    }

    pub fn heal(&mut self, n: i32) {
        self.hp = (self.hp + n).min(self.max_hp);
    }

    pub fn add_relic(&mut self, r: Relic) {
        if r == Relic::BoneIdol {
            self.max_hp += 10;
            self.hp += 10;
        }
        self.relics.push(r);
    }

    pub fn random_relic(&mut self) -> Option<Relic> {
        let avail: Vec<Relic> = Relic::ALL
            .iter()
            .copied()
            .filter(|r| !self.relics.contains(r))
            .collect();
        if avail.is_empty() {
            None
        } else {
            Some(*self.rng.pick(&avail))
        }
    }

    pub fn random_card(&mut self, rare_bias: f32) -> Card {
        let roll = self.rng.f32();
        let want = if roll < 0.10 + rare_bias {
            Rarity::Rare
        } else if roll < 0.40 + rare_bias * 2.0 {
            Rarity::Uncommon
        } else {
            Rarity::Common
        };
        let pool = class_pool(self.class);
        let matching: Vec<_> = pool
            .iter()
            .copied()
            .filter(|&id| Card::new(id).def().rarity == want)
            .collect();
        let id = if matching.is_empty() {
            *self.rng.pick(&pool)
        } else {
            *self.rng.pick(&matching)
        };
        let up = self.rng.chance(0.04 * self.floor as f32);
        Card { id, up }
    }

    /// `n` distinct reward cards.
    pub fn reward_cards(&mut self, n: usize, rare_bias: f32) -> Vec<Card> {
        let mut out: Vec<Card> = Vec::new();
        let mut tries = 0;
        while out.len() < n && tries < 50 {
            tries += 1;
            let c = self.random_card(rare_bias);
            if !out.iter().any(|o| o.id == c.id) {
                out.push(c);
            }
        }
        out
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = save_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)
    }

    pub fn load() -> Option<RunState> {
        let data = std::fs::read_to_string(save_path()).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn delete_save() {
        let _ = std::fs::remove_file(save_path());
    }

    pub fn has_save() -> bool {
        save_path().exists()
    }
}

pub fn save_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("crypts_of_the_unholy").join("save.json")
}

fn gen_map(rng: &mut Rng) -> Vec<Vec<RoomKind>> {
    use RoomKind::*;
    let mut map = Vec::with_capacity(FLOORS);
    for floor in 0..FLOORS {
        let rooms = match floor {
            0 => vec![Combat],
            8 => vec![Shrine],
            9 => vec![Boss],
            _ => {
                let n = if rng.chance(0.5) { 3 } else { 2 };
                let mut rooms: Vec<RoomKind> = Vec::new();
                // Guarantee a fight on most floors so the deck can grow.
                if floor != 4 || rng.chance(0.5) {
                    rooms.push(if floor >= 3 && rng.chance(0.3) {
                        Elite
                    } else {
                        Combat
                    });
                }
                while rooms.len() < n {
                    let r = roll_room(floor, rng);
                    if !rooms.contains(&r) {
                        rooms.push(r);
                    }
                }
                rng.shuffle(&mut rooms);
                rooms
            }
        };
        map.push(rooms);
    }
    map
}

fn roll_room(floor: usize, rng: &mut Rng) -> RoomKind {
    use RoomKind::*;
    let table: &[(RoomKind, u32)] = &[
        (Combat, 30),
        (Elite, 14),
        (Shrine, 14),
        (Treasure, 12),
        (Merchant, 14),
        (Event, 18),
    ];
    loop {
        let total: u32 = table.iter().map(|t| t.1).sum();
        let mut roll = (rng.next_u64() % total as u64) as u32;
        for &(room, w) in table {
            if roll < w {
                if room == Elite && floor < 3 {
                    break;
                }
                return room;
            }
            roll -= w;
        }
    }
}
