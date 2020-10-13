use crate::comms::{CommsMessage, CommsVerifier};
use crate::primitives::{Account, AccountType, ChallengeStatus, NetAccount, Result};
use crate::Database2;
use strsim::jaro;

pub struct DisplayNameHandler {
    db: Database2,
    comms: CommsVerifier,
    limit: f64,
}

impl DisplayNameHandler {
    pub fn new(db: Database2, comms: CommsVerifier, limit: f64) -> Self {
        DisplayNameHandler {
            db: db,
            comms: comms,
            limit: limit,
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
            } => {
                self.handle_display_name_matching(net_account, account)
                    .await?
            }
            _ => error!("Received unrecognized message type"),
        }

        Ok(())
    }
    pub async fn handle_display_name_matching(
        &self,
        net_account: NetAccount,
        account: Account,
    ) -> Result<()> {
        let display_names = self.db.select_display_names().await?;
        let mut violations = vec![];

        for display_name in &display_names {
            if self.is_too_similar(display_name, &account).await {
                violations.push(display_name);
            }

            // Cap this at 5 violations, prevent sending oversized buffers.
            if violations.len() == 5 {
                break;
            }
        }

        if violations.is_empty() {
            self.db
                .set_challenge_status(
                    &net_account,
                    &AccountType::DisplayName,
                    &ChallengeStatus::Accepted,
                )
                .await?;
        } else {
            self.db
                .set_challenge_status(
                    &net_account,
                    &AccountType::DisplayName,
                    &ChallengeStatus::Accepted,
                )
                .await?;
        }

        self.comms.notify_status_change(net_account);

        Ok(())
    }
    async fn is_too_similar(&self, display_name: &Account, account: &Account) -> bool {
        let name_str = display_name.as_str().to_lowercase();
        let account_str = account.as_str().to_lowercase();

        let similarities = [
            jaro(&name_str, &account_str),
            jaro_words(&name_str, &account_str, " "),
            jaro_words(&name_str, &account_str, "-"),
            jaro_words(&name_str, &account_str, "_"),
        ];

        similarities.iter().any(|&s| s > self.limit)
    }
}

fn jaro_words(left: &str, right: &str, delimiter: &str) -> f64 {
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
