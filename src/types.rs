use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng, Rng,
};
use serde::{Deserialize, Serialize};
use worker::{console_log, Date};

use crate::notification::Notification;


// #[derive(Serialize, Deserialize, Debug, Clone)]
// pub struct Notification {
//     pub id: String,          // unique id for each notification
//     pub kind: NotificationType, // type of notification
//     pub message: String,     // text message
//     pub timestamp: u64,      // unix time (in seconds)
//     pub read: bool,          // whether the notification was read
// }

// #[derive(Serialize, Deserialize, Debug, Clone)]
// pub enum NotificationType {
//     Referral,
//     ConsensusResult,
//     GameUpdate,
// }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Op {
    CombineAlien(usize, usize),
    SpawnAlien,
    DeleteAlienFromInventory(usize),
    DeleteAlienFromActive(usize),
    UsePowerup(usize),
    SpawnPowerup(PowerUpKind),
    GetData,
    Register,
    AwardBadge(BadgesKind),
    UpdateEmail(String),
    UpdatePfp(Option<String>),
    UpdateLastLogin(u64),
    UpdateIq(usize),
    UpdateSocialScore(usize),
    IncrementAkaiBalance,
    DecrementAkaiBalance,
    IncrementTotalTaskCompleted,
    IncrementPlayersReferred,
    UpdateLeague(LeagueType),
    UpdateAllTaskDone(bool),
    MoveAlienFromInventoryToActive,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WsMsg {
    pub user_id: String,
    pub op: Op,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
pub enum PowerUpKind {
    RowPowerUp,
    ColumnPowerUp,
    NearestSquarePowerUp,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum BadgesKind {
    TenTaskBadge,
    TwentyTaskBadge,
    ThirtyTaskBadge,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub enum LeagueType {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    GrandMaster,
    Challenger,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct LeaderboardData {
    pub league: usize,
    pub global: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct UserProfile {
    pub user_id: String,
    pub email: Option<String>,
    pub pfp: Option<String>,
    pub last_login: u64,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct GameState {
    pub active_aliens: [usize; 16],
    pub inventory_aliens: Vec<usize>,
    pub power_ups: Vec<PowerUpKind>,
    pub king_lvl: usize,
    pub total_merged_aliens: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct Progress {
    pub iq: usize,
    pub social_score: usize,
    pub product: usize,
    pub all_task_done: bool,
    pub akai_balance: usize,
    pub total_task_completed: usize,
    pub streak: usize,
    pub badges: Vec<BadgesKind>,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct SocialData {
    pub players_referred: usize,
    pub referal_code: String,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct UserData {
    pub profile: UserProfile,
    pub game_state: GameState,
    pub progress: Progress,
    pub social: SocialData,
    pub league: LeagueType,
    pub notifications: Vec<Notification>, // <-- added this
}

impl Default for UserData {
    fn default() -> Self {
        console_log!("defaulting user data");
        let mut res = Self {
            profile: UserProfile {
                user_id:
                 Alphanumeric.sample_string(&mut thread_rng(), 32),
                email: None,
                pfp: None,
                last_login: Date::now().as_millis() / 1000,
            },
            game_state: GameState {
                active_aliens: [0; 16],
                inventory_aliens: Vec::new(),
                power_ups: Vec::new(),
                king_lvl: 0,
                total_merged_aliens: 0,
            },
            progress: Progress {
                iq: 0,
                social_score: 0,
                all_task_done: false,
                product: 0,
                akai_balance: 0,
                total_task_completed: 0,
                streak: 0,
                badges: Vec::new(),
            },
            social: SocialData {
                players_referred: 0,
                referal_code: 
                thread_rng()
                    .sample_iter(Alphanumeric)
                    .take(8)
                    .map(|b| b as char)
                    .collect(),
            },
            league: LeagueType::Bronze,
            notifications: Vec::new(), // <-- added this
        };

        for i in 0..5{
            res.game_state.active_aliens[i] = 1;
        }

        res
    }
}

impl UserData {
    pub fn calculate_last_login(&mut self) {
        console_log!("calculating streak");
        let current_time = Date::now().as_millis() / 1000;
        console_log!("current time: {:?}", current_time);
        let time_since_last_login = current_time - self.profile.last_login;
        let one_day = 60 * 60 * 24;
        let two_days = one_day * 2;

        if time_since_last_login > one_day && time_since_last_login < two_days {
            self.progress.streak += 1;
            self.profile.last_login = current_time;
        } else if time_since_last_login >= two_days {
            self.progress.streak = 0;
            self.profile.last_login = current_time;
        }
    }

    pub fn mark_notification_read(&mut self, notification_id: &str) {
        if let Some(notification) = self.notifications.iter_mut().find(|n| n.notification_id == notification_id) {
            notification.read = true;
        }
    }
}
