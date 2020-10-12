use crate::comms::{CommsMessage, CommsVerifier};
use crate::primitives::{Account, Result};
use crate::Database2;
use std::collections::HashMap;

pub struct StringMatcher {
    db: Database2,
    comms: CommsVerifier,
    similarity_limit: f64,
}

impl StringMatcher {
    pub fn new(db: Database2, comms: CommsVerifier, similarity_limit: f64) -> Self {
        StringMatcher {
            db: db,
            comms: comms,
            similarity_limit: similarity_limit,
        }
    }
    pub async fn start(self) {
        loop {
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
                err
            });
        }
    }
    pub async fn local(&self) -> Result<()> {
        use CommsMessage::*;

        match self.comms.recv().await {
            AccountToVerify {
                net_account,
                account,
            } => self.handle_display_name_matching(&account).await?,
            _ => error!("Received unrecognized message type"),
        }

        Ok(())
    }
    pub async fn handle_display_name_matching(&self, account: &Account) -> Result<()> {
        let display_names = self.db.select_display_names().await?;

        Ok(())
    }
    fn match_strings<'a>(
        display_names: &'a [Account],
        account: &Account,
        similarity_limit: f64,
    ) -> Vec<&'a Account> {
        let mut too_close = vec![];

        for display_name in display_names {
            let name_str = display_name.as_str().to_lowercase();
            let account_str = account.as_str().to_lowercase();

            println!("{} - {}", account.as_str(), display_name.as_str());
            println!("  jaro: {}", strsim::jaro(&name_str, &account_str));
            println!(
                "  jaro-winkler: {}",
                strsim::jaro_winkler(&name_str, &account_str)
            );
            println!("  chars: {}", char_counter(&name_str, &account_str));
            println!("  words: {}", match_words(&name_str, &account_str, " "));
            println!("");
        }

        too_close
    }
}

fn match_words(left: &str, right: &str, delimiter: &str) -> f64 {
    let left_words: Vec<&str> = left
        .split(delimiter)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let right_words: Vec<&str> = right
        .split(delimiter)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut total = 0.0;

    for left_word in &left_words {
        let mut temp = 0.0;

        for right_word in &right_words {
            let sim = strsim::jaro(left_word, right_word);

            if sim > temp {
                temp = sim;
            }
        }

        total += temp;
    }

    total as f64 / left_words.len().max(right_words.len()) as f64
}

fn char_counter(left: &str, right: &str) -> f64 {
    fn counter(string: &str) -> HashMap<char, i32> {
        let mut map = HashMap::new();

        for char in string.chars() {
            map.entry(char).and_modify(|c| *c += 1).or_insert(1);
        }

        map
    }

    let left_chars = counter(left);
    let right_chars = counter(right);

    let mut matches = 0;

    for (left_char, left_count) in left_chars.iter() {
        if let Some(right_count) = right_chars.get(left_char) {
            if left_count == right_count {
                matches += 1;
            }
        }
    }

    let total = left_chars.len().max(right_chars.len());

    matches as f64 / total as f64
}

#[test]
fn match_strings() {
    let account = Account::from("John");

    #[rustfmt::skip]
    let display_names = [
        "John",
        "john",
        "joHn",
        "jOHN",
        "JohN",
        "Jonh",
        "Jnoh",
        "Johnie",
        "Johnnie",
        "Johnnie Cage",
        "Jack",
        "Jax",
        "Jaxon",
        "nhoJ",
        "Mike",
    ];

    let display_names = display_names
        .iter()
        .map(|&name| Account::from(name))
        .collect::<Vec<Account>>();

    let _ = StringMatcher::match_strings(&display_names, &account, 1.0);

    let account = Account::from("Anson & Fabio");

    #[rustfmt::skip]
    let display_names = [
        "Anson & Fabio",
        "Ansno & Fabio",
        "Anosn & Faibo",
        "Fabio & Anson",
        "Jeff & Fabio",
        "Anson & Jeff",
    ];

    let display_names = display_names
        .iter()
        .map(|&name| Account::from(name))
        .collect::<Vec<Account>>();

    let _ = StringMatcher::match_strings(&display_names, &account, 1.0);
}
