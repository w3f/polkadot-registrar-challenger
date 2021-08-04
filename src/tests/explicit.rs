use super::*;
use crate::primitives::JudgementState;

#[actix::test]
async fn default_state() {
    let alice = JudgementState::alice();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
	assert_eq!(alice.get_field(&F::ALICE_DISPLAY_NAME()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(), false);
}

#[actix::test]
async fn set_valid() {
    let mut alice = JudgementState::alice();

	alice.get_field_mut(&F::ALICE_MATRIX()).expected_message_mut().set_verified();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
	assert_eq!(alice.get_field(&F::ALICE_DISPLAY_NAME()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(), false);
	// Verified
	assert_eq!(alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(), true);
	assert_eq!(alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(), false);
}

#[actix::test]
async fn set_valid_second_challenge() {
    let mut alice = JudgementState::alice();

	alice.get_field_mut(&F::ALICE_EMAIL()).expected_message_mut().set_verified();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
	assert_eq!(alice.get_field(&F::ALICE_DISPLAY_NAME()).challenge.is_verified(), false);
	// Not verified, second challenge missing.
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(), false);

	// Individual checks.
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).expected_message().is_verified, true);
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).expected_second().is_verified, false);

	// Set second challenge to verified.
	alice.get_field_mut(&F::ALICE_EMAIL()).expected_second_mut().set_verified();

    assert_eq!(alice.is_fully_verified, false);
    assert_eq!(alice.completion_timestamp, None);
    assert_eq!(alice.judgement_submitted, false);
    assert_eq!(alice.issue_judgement_at, None);
	assert_eq!(alice.get_field(&F::ALICE_DISPLAY_NAME()).challenge.is_verified(), false);
	// Verified.
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).challenge.is_verified(), true);
	assert_eq!(alice.get_field(&F::ALICE_MATRIX()).challenge.is_verified(), false);
	assert_eq!(alice.get_field(&F::ALICE_TWITTER()).challenge.is_verified(), false);

	// Individual checks.
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).expected_message().is_verified, true);
	assert_eq!(alice.get_field(&F::ALICE_EMAIL()).expected_second().is_verified, true);
}
