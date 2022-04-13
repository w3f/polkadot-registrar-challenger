// Makes it easier to read.
#![allow(clippy::bool_assert_comparison)]

use super::*;
use crate::primitives::JudgementState;

#[actix::test]
async fn default_state() {
    let alice = JudgementState::alice();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
    assert_eq!(
        alice
            .get_field(&F::ALICE_DISPLAY_NAME())
            .challenge
            .is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(),
        false
    );
}

#[actix::test]
async fn set_verified() {
    let mut alice = JudgementState::alice();

    alice
        .get_field_mut(&F::ALICE_MATRIX())
        .expected_message_mut()
        .set_verified();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
    assert_eq!(
        alice
            .get_field(&F::ALICE_DISPLAY_NAME())
            .challenge
            .is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(),
        false
    );
    // Verified
    assert_eq!(
        alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(),
        true
    );
    assert_eq!(
        alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(),
        false
    );
    assert_eq!(alice.check_full_verification(), false);
}

#[actix::test]
async fn set_valid_second_challenge() {
    let mut alice = JudgementState::alice();

    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_message_mut()
        .set_verified();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
    assert_eq!(
        alice
            .get_field(&F::ALICE_DISPLAY_NAME())
            .challenge
            .is_verified(),
        false
    );
    // Not verified, second challenge missing.
    assert_eq!(
        alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(),
        false
    );
    assert_eq!(alice.check_full_verification(), false);

    // Individual checks.
    assert_eq!(
        alice
            .get_field(&F::ALICE_EMAIL())
            .expected_message()
            .is_verified,
        true
    );
    assert_eq!(
        alice
            .get_field(&F::ALICE_EMAIL())
            .expected_second()
            .is_verified,
        false
    );

    // Set second challenge to verified.
    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_second_mut()
        .set_verified();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
    assert_eq!(
        alice
            .get_field(&F::ALICE_DISPLAY_NAME())
            .challenge
            .is_verified(),
        false
    );
    // Verified.
    assert_eq!(
        alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(),
        true
    );
    assert_eq!(
        alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(),
        false
    );
    assert_eq!(
        alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(),
        false
    );
    assert_eq!(alice.check_full_verification(), false);

    // Individual checks.
    assert_eq!(
        alice
            .get_field(&F::ALICE_EMAIL())
            .expected_message()
            .is_verified,
        true
    );
    assert_eq!(
        alice
            .get_field(&F::ALICE_EMAIL())
            .expected_second()
            .is_verified,
        true
    );
}

#[actix::test]
async fn set_verified_all() {
    let mut alice = JudgementState::alice();

    *alice
        .get_field_mut(&F::ALICE_DISPLAY_NAME())
        .expected_display_name_check_mut()
        .0 = true;
    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_message_mut()
        .set_verified();
    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_second_mut()
        .set_verified();
    alice
        .get_field_mut(&F::ALICE_MATRIX())
        .expected_message_mut()
        .set_verified();
    alice
        .get_field_mut(&F::ALICE_TWITTER())
        .expected_message_mut()
        .set_verified();

    // This field must be adjusted manually.
    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
    assert_eq!(
        alice
            .get_field(&F::ALICE_DISPLAY_NAME())
            .challenge
            .is_verified(),
        true
    );
    assert_eq!(
        alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(),
        true
    );
    assert_eq!(
        alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(),
        true
    );
    assert_eq!(
        alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(),
        true
    );
    // All fields are verified.
    assert_eq!(alice.check_full_verification(), true);
}
