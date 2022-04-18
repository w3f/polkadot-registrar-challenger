CREATE TABLE judgement (
	id					SERIAL PRIMARY KEY,
	address				TEXT NOT NULL,
	network_id			TEXT NOT NULL,
	is_fully_verified	BOOLEAN NOT NULL DEFAULT FALSE,
	judgement_submitted BOOLEAN NOT NULL DEFAULT FALSE,
	inserted_timestamp	TIMESTAMP NOT NULL DEFAULT current_timestamp,
	completed_timestamp TIMESTAMP DEFAULT NULL,

	FOREIGN KEY (network_id)
		REFERENCES network (id),

	UNIQUE (address, network_id)
);

CREATE TABLE network (
	id SERIAL PRIMARY KEY,
	name TEXT NOT NULL,

	UNIQUE (name)
);

CREATE TABLE account (
	id 					SERIAL PRIMARY KEY,
	judgement_id 		BIGINT NOT NULL,
	account_ty_id		BIGINT NOT NULL,
	name 				TEXT NOT NULL,
	is_verified 		BOOLEAN NOT NULL DEFAULT FALSE,
	verified_timestamp 	TIMESTAMP DEFAULT NULL,
	failed_attempts: 	BIGINT NOT NULL DEFAULT 0,

	FOREIGN KEY (judgement_id)
		REFERENCES judgement (id),
	FOREIGN KEY (account_ty_id)
		REFERENCES account_type (id),

	UNIQUE (judgement_id, account_ty_id)
)

CREATE TABLE account_type (
	id 		SERIAL PRIMARY KEY,
	name 	TEXT NOT NULL
)

CREATE TABLE challenge_expected_message (
	id 					SERIAL PRIMARY KEY,
	account_id			BIGINT NOT NULL,
	first_challenge 	TEXT NOT NULL,
	second_challenge 	TEXT NOT NULL,
	is_first_verified 	BOOLEAN NOT NULL DEFAULT FALSE,
	is_second_verified 	BOOLEAN NOT NULL DEFAULT FALSE

	FOREIGN KEY (account_id)
		REFERENCES account (id),

	UNIQUE (account_id)
)

CREATE TABLE challenge_display_name_check (
	id 				SERIAL PRIMARY KEY,
	account_id		BIGINT NOT NULL,
	is_verified 	BOOLEAN NOT NULL DEFAULT FALSE,
	violations 		TEXT[] DEFAULT NULL

	FOREIGN KEY (account_id)
		REFERENCES account (id),

	UNIQUE (account_id)
)

CREATE TABLE challenge_manual_verification (
	id 				SERIAL PRIMARY KEY,
	account_id		BIGINT NOT NULL,
	is_verified 	BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (account_id)
		REFERENCES account (id),

	UNIQUE (account_id)
)
