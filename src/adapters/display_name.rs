use crate::comms::{CommsMessage, CommsVerifier};
use crate::manager::AccountStatus;
use crate::primitives::{Account, AccountType, ChallengeStatus, NetAccount, Result};
use crate::Database;
use strsim::jaro;

pub const VIOLATIONS_CAP: usize = 5;

pub struct DisplayNameHandler {
    db: Database,
    comms: CommsVerifier,
    limit: f64,
}

impl DisplayNameHandler {
    pub fn new(db: Database, comms: CommsVerifier, limit: f64) -> Self {
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
        let display_names = self.db.select_display_names(&net_account).await?;
        let mut violations = vec![];

        for display_name in &display_names {
            if Self::is_too_similar(display_name, &account, self.limit) {
                violations.push(display_name.clone());
            }

            // Cap the violation list, prevent sending oversized buffers.
            if violations.len() == VIOLATIONS_CAP {
                break;
            }
        }

        self.db.delete_display_name_violations(&net_account).await?;

        // The display name does obviously not need to be verified by
        // signing a challenge or having to contact an address. But we just
        // treat it as any other "account".
        if violations.is_empty() {
            // Keep track of display names for future matching.
            self.db
                .insert_display_name(Some(&net_account), &account)
                .await?;

            self.db
                .set_account_status(
                    &net_account,
                    &AccountType::DisplayName,
                    &AccountStatus::Valid,
                )
                .await?;

            self.db
                .set_challenge_status(
                    &net_account,
                    &AccountType::DisplayName,
                    &ChallengeStatus::Accepted,
                )
                .await?;
        } else {
            self.db
                .insert_display_name_violations(&net_account, &violations)
                .await?;

            self.db
                .set_account_status(
                    &net_account,
                    &AccountType::DisplayName,
                    &AccountStatus::Invalid,
                )
                .await?;

            self.db
                .set_challenge_status(
                    &net_account,
                    &AccountType::DisplayName,
                    &ChallengeStatus::Rejected,
                )
                .await?;
        }

        self.comms.notify_status_change(net_account);

        Ok(())
    }
    fn is_too_similar(display_name: &Account, account: &Account, limit: f64) -> bool {
        let name_str = display_name.as_str().to_lowercase();
        let account_str = account.as_str().to_lowercase();

        #[cfg(test)]
        {
            let jwinkler = jaro(&name_str, &account_str);
            let jwords = jaro_words(&name_str, &account_str, &[" ", "-", "_"]);

            println!("{} == {} (?)", account, display_name);
            println!("  - jaro: {}", jwinkler);
            println!("  - jaro_words: {}", jwords);
        }

        let similarities = [
            jaro(&name_str, &account_str),
            jaro_words(&name_str, &account_str, &[" ", "-", "_"]),
        ];

        similarities.iter().any(|&s| s > limit)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Account;

    const LIMIT: f64 = 0.85;

    #[test]
    fn is_too_similar() {
        let current = [
            Account::from("dave"),
            Account::from("Dave"),
            Account::from("daev"),
            Account::from("Daev"),
        ];

        let new = Account::from("dave");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("David"),
            Account::from("alice"),
            Account::from("Alice"),
            Account::from("bob"),
            Account::from("Bob"),
            Account::from("eve"),
            Account::from("Eve"),
            Account::from("David"),
        ];

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_words() {
        let current = [
            Account::from("adam & eve"),
            Account::from("Adam & Eve"),
            Account::from("aadm & Eve"),
            Account::from("Aadm & Eve"),
            Account::from("adam & ev"),
            Account::from("Adam & Ev"),
            Account::from("eve & adam"),
            Account::from("Eve & Adam"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("alice & bob"),
            Account::from("Alice & Bob"),
            Account::from("jeff & john"),
            Account::from("Jeff & John"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_words_special_delimiter() {
        let current = [
            Account::from("adam & eve"),
            Account::from("Adam & Eve"),
            Account::from("aadm & Eve"),
            Account::from("Aadm & Eve"),
            Account::from("adam & ev"),
            Account::from("Adam & Ev"),
            Account::from("eve & adam"),
            Account::from("Eve & Adam"),
            //
            Account::from("adam-&-eve"),
            Account::from("Adam-&-Eve"),
            Account::from("aadm-&-Eve"),
            Account::from("Aadm-&-Eve"),
            Account::from("adam-&-ev"),
            Account::from("Adam-&-Ev"),
            Account::from("eve-&-adam"),
            Account::from("Eve-&-Adam"),
            //
            Account::from("adam_&_eve"),
            Account::from("Adam_&_Eve"),
            Account::from("aadm_&_Eve"),
            Account::from("Aadm_&_Eve"),
            Account::from("adam_&_ev"),
            Account::from("Adam_&_Ev"),
            Account::from("eve_&_adam"),
            Account::from("Eve_&_Adam"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("alice & bob"),
            Account::from("Alice & Bob"),
            Account::from("jeff & john"),
            Account::from("Jeff & John"),
            //
            Account::from("alice_&_bob"),
            Account::from("Alice_&_Bob"),
            Account::from("jeff_&_john"),
            Account::from("Jeff_&_John"),
            //
            Account::from("alice-&-bob"),
            Account::from("Alice-&-Bob"),
            Account::from("jeff-&-john"),
            Account::from("Jeff-&-John"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_unicode() {
        let current = [
            Account::from("👻🥺👌 Alice"),
        ];

        let new = Account::from("👻🥺👌 Alice");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("Alice"),
            Account::from("👻🥺👌 Johnny 💀"),
            Account::from("🤖👈👈 Alice"),
            Account::from("👻🥺👌 Bob"),
            Account::from("👻🥺👌 Eve"),
        ];

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }
}
