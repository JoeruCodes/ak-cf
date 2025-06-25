use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Hint {
    pub id: usize,
    pub category: HintCategory,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum HintCategory {
    GameStrategy,
    SpaceFact,
    AlienTip,
    PowerUpTip,
}

impl Hint {
    pub fn new(id: usize, category: HintCategory, content: String) -> Self {
        Self {
            id,
            category,
            content,
        }
    }
}

pub fn get_all_hints() -> Vec<Hint> {
    vec![
        Hint::new(
            1,
            HintCategory::GameStrategy,
            "Combine lower-level aliens first to create space for new ones!".to_string(),
        ),
        Hint::new(
            2,
            HintCategory::SpaceFact,
            "The largest known star, UY Scuti, is so big that 1.7 billion Suns could fit inside it!".to_string(),
        ),
        Hint::new(
            3,
            HintCategory::AlienTip,
            "Higher-level aliens give more points when merged. Plan your combinations wisely!".to_string(),
        ),
        Hint::new(
            4,
            HintCategory::PowerUpTip,
            "Row and Column power-ups can clear multiple aliens at once. Save them for strategic moments!".to_string(),
        ),
        Hint::new(
            5,
            HintCategory::SpaceFact,
            "A day on Venus is longer than its year! Venus takes 243 Earth days to rotate but only 225 Earth days to orbit the Sun.".to_string(),
        ),
        Hint::new(
            6,
            HintCategory::GameStrategy,
            "Keep your grid organized! Try to group similar aliens together for easier merging.".to_string(),
        ),
        Hint::new(
            7,
            HintCategory::AlienTip,
            "The King Alien's level affects the strength of new aliens you receive. Upgrade it regularly!".to_string(),
        ),
        Hint::new(
            8,
            HintCategory::SpaceFact,
            "There are more stars in the universe than grains of sand on all Earth's beaches combined!".to_string(),
        ),
        Hint::new(
            9,
            HintCategory::PowerUpTip,
            "Nearest Square power-up targets the closest alien. Use it when you need to clear a specific area!".to_string(),
        ),
        Hint::new(
            10,
            HintCategory::GameStrategy,
            "Complete daily tasks to earn rewards and boost your progress faster!".to_string(),
        ),
    ]
}

pub fn get_random_hint() -> Hint {
    let hints = get_all_hints();
    let mut rng = thread_rng();
    hints.choose(&mut rng).unwrap().clone()
}

pub fn get_hint_by_id(id: usize) -> Option<Hint> {
    let hints = get_all_hints();
    hints.into_iter().find(|h| h.id == id)
}
