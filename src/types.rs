use chrono::Utc;
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng, Rng,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use worker::{console_log, Date};

use crate::notification::{Notification, Read};
use crate::{
    daily_task::{Links, SocialPlatform},
    notification::NotificationType,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Op {
    CombineAlien(usize, usize),
    MoveAlienFromInventoryToActive,
    DeleteAlienFromActive(usize),
    MoveAlienInGrid(usize, usize),
    SpawnPowerup(PowerUpKind), //should remove after testing
    UsePowerup(usize, usize),  //changed
    GetData,
    Register(String),
    UpdateEmail(String),
    UpdatePfp(usize),
    UpdateUserName(Option<String>),
    UpdatePassword(String),
    AddNotificationInternal(Notification),
    MarkNotificationRead(String),
    ProcessNotificationMetadata(String),
    UseReferralCode(String),
    GenerateDailyTasks,
    CheckDailyTask(Option<String>),
    ClaimDailyReward(usize),
    SyncData,
    SubmitMcqAnswers(String, Vec<String>), // (datapoint_id, answers)
    SubmitTextAnswer(String, usize, String), // (datapoint_id, idx, text)
    PingPong,                              // New operation for WebSocket ping/pong
    GetAvailableCryptos,                   // Get list of available cryptos for user
    ExchangeAkaiForCrypto(usize, String, String), // Exchange akai for crypto
    GetCryptoExchangeAmount(usize, String), // akai_amount, crypto_symbol
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WsMsg {
    pub op: Op,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DurableObjectAugmentedMsg {
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

impl LeagueType {
    pub fn from_product(product: usize) -> Self {
        match product / 50 {
            0 => LeagueType::Bronze,
            1 => LeagueType::Silver,
            2 => LeagueType::Gold,
            3 => LeagueType::Platinum,
            4 => LeagueType::Diamond,
            5 => LeagueType::Master,
            6 => LeagueType::GrandMaster,
            _ => LeagueType::Challenger,
        }
    }
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
    pub pfp: usize,
    pub user_name: Option<String>,
    pub password: Option<String>,
    pub last_login: u64,
    pub real_login: u64,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct GameState {
    pub active_aliens: [usize; 16],
    pub inventory_aliens: usize,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct Question {
    pub q: String,
    pub a: String,
    pub textAnswers: Vec<String>,
    pub mcqAnswers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McqPreLabel {
    pub map_placement: String,
    pub questions: Vec<Question>,
    pub summary: String,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct McqVideoTask {
    #[serde(rename = "_id")]
    pub id: String,
    pub task_id: String,
    pub mediaUrl: String,
    pub preLabel: McqPreLabel,
    pub visited: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct TextVideoTask {
    pub datapointId: String,
    pub questionIndex: usize,
    pub question: String,
    pub mediaUrl: String,
    pub visited: bool,
    pub map_placement: String,
    pub keywords: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct DailyProgress {
    pub links: Vec<Links>,
    pub daily_merge: (usize, usize, bool),
    pub daily_annotate: (usize, usize, bool),
    pub daily_powerups: (usize, usize, bool),
    pub total_completed: usize,
    pub alien_earned: Option<usize>,
    pub pu_earned: Option<PowerUpKind>,
    pub mcq_video_tasks: Vec<McqVideoTask>,
    pub text_video_tasks: Vec<TextVideoTask>,
    pub last_task_generation: u64,
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
    pub daily: DailyProgress,
}

impl Default for UserData {
    fn default() -> Self {
        console_log!("defaulting user data");
        let mut res = Self {
            profile: UserProfile {
                user_id: "kunal".to_string(),
                email: None,
                pfp: 1,
                user_name: None,
                password: Some("123456".to_string()),
                last_login: Date::now().as_millis() / 1000,
                real_login: Date::now().as_millis() / 1000,
            },
            game_state: GameState {
                active_aliens: [0; 16],
                inventory_aliens: 30,
                power_ups: vec![
                    PowerUpKind::ColumnPowerUp,
                    PowerUpKind::RowPowerUp,
                    PowerUpKind::NearestSquarePowerUp,
                ],
                king_lvl: 1,
                total_merged_aliens: 0,
            },
            progress: Progress {
                iq: 40,
                social_score: 20,
                all_task_done: false,
                product: 0,
                akai_balance: 95,
                total_task_completed: 0,
                streak: 0,
                badges: Vec::new(),
            },
            social: SocialData {
                players_referred: 0,
                referal_code: thread_rng()
                    .sample_iter(Alphanumeric)
                    .take(8)
                    .map(|b| b as char)
                    .collect(),
            },
            league: LeagueType::Bronze,
            notifications: Vec::new(), // <-- added this,
            daily: DailyProgress {
                links: Vec::new(),
                mcq_video_tasks: Vec::new(),
                text_video_tasks: Vec::new(),
                daily_merge: (0, 0, false),
                daily_annotate: (0, 0, false),
                daily_powerups: (0, 0, false),
                total_completed: 0,
                alien_earned: None,
                pu_earned: None,
                last_task_generation: 0,
            },
        };

        for i in 0..10 {
            res.game_state.active_aliens[i] = 1;
        }

        res.notifications.push(Notification {
            notification_id: Uuid::new_v4().to_string(),
            user_id: res.profile.user_id.clone(),
            notification_type: NotificationType::System,
            message: "Welcome to the game!".to_string(),
            timestamp: Utc::now().timestamp(),
            read: Read::No,
            metadata: None, // 👈 No metadata
        });

        res
    }
}

impl UserData {
    pub fn calculate_last_login(&mut self) -> Option<Reward> {
        // Streak Calculation Logic
        console_log!("calculating streak");
        let current_time = Date::now().as_millis() / 1000;
        let time_since_last_login = current_time - self.profile.last_login;
        let one_day = 60 * 60 * 24;
        let two_days = one_day * 2;

        if time_since_last_login > one_day && time_since_last_login < two_days {
            self.progress.streak += 1;
            self.profile.last_login = current_time;
            Some(self.get_daily_login_rewards())
        } else if time_since_last_login >= two_days {
            self.progress.streak = 0;
            self.profile.last_login = current_time;
            Some(self.get_daily_login_rewards())
        } else {
            None
        }
    }

    pub fn get_daily_login_rewards(&mut self) -> Reward {
        // Give daily login rewards
        self.progress.akai_balance += 20;

        // Give 2 random power-ups
        let powerup1 = crate::utils::give_random_power_up(self);
        let powerup2 = crate::utils::give_random_power_up(self);

        let mut reward = Reward {
            is_reward: true,
            rewards: HashMap::new(),
        };
        reward
            .rewards
            .insert("akai_balance".to_string(), "20".to_string());
        reward
            .rewards
            .insert("powerup1".to_string(), format!("{:?}", powerup1));
        reward
            .rewards
            .insert("powerup2".to_string(), format!("{:?}", powerup2));
        if self.progress.streak > 0 {
            reward
                .rewards
                .insert("streak".to_string(), self.progress.streak.to_string());
        }

        console_log!(
            "Daily login rewards:  +20 Akai,  +2 powerups, streak: {}",
            self.progress.streak
        );
        reward
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Reward {
    pub is_reward: bool,
    pub rewards: HashMap<String, String>,
}

/// Info about a supported crypto
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CryptoInfo {
    pub symbol: String,
    pub name: String,
    pub network: String,
    pub rpc_url: String,
    pub min_iq: usize,
    pub api_id: String, // CoinGecko API ID
    pub contract_address: Option<String>,
    pub decimals: u8,
}





#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ExchangeAkaiForCrypto {
    pub tx_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CryptoExchangeAmount {
    pub amount: f64,
}

