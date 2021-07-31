use crate::database::Database;
use crate::primitives::JudgementState;
use crate::Result;
use strsim::jaro;

const VIOLATIONS_CAP: usize = 5;

pub struct DisplayNameVerifier {
    db: Database,
    limit: f64,
}

impl DisplayNameVerifier {
    pub async fn verify_display_name(&self, state: JudgementState) -> Result<()> {
        let name = if let Some(name) = state.display_name() {
            name
        } else {
            return Ok(())
        };

        let current = self.db.fetch_display_names().await?;
        let mut violations = vec![];

        for existing in current {
            if is_too_similar(&name, &existing.display_name, self.limit) {
                // Only show up to `VIOLATIONS_CAP` violations.
                if violations.len() == VIOLATIONS_CAP {
                    break;
                }

                violations.push(existing);
            }
        }



        Ok(())
    }
}

fn is_too_similar(existing: &str, new: &str, limit: f64) -> bool {
    let name_str = existing.to_lowercase();
    let account_str = new.to_lowercase();

    let similarities = [
        jaro(&name_str, &account_str),
        jaro_words(&name_str, &account_str, &[" ", "-", "_"]),
    ];

    similarities.iter().any(|&s| s > limit)
}

fn jaro_words(left: &str, right: &str, delimiter: &[&str]) -> f64 {
    fn splitter<'a>(string: &'a str, delimiter: &[&str]) -> Vec<&'a str> {
        let mut all = vec![];

        for del in delimiter {
            let mut words: Vec<&str> = string
                .split(del)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            all.append(&mut words);
        }

        all
    }

    let left_words = splitter(left, delimiter);
    let right_words = splitter(right, delimiter);

    let mut total = 0.0;

    for left_word in &left_words {
        let mut temp = 0.0;

        for right_word in &right_words {
            let sim = jaro(left_word, right_word);

            if sim > temp {
                temp = sim;
            }
        }

        total += temp;
    }

    total as f64 / left_words.len().max(right_words.len()) as f64
}
