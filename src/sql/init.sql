CREATE TABLE network (
	id SERIAL PRIMARY KEY,
	name TEXT NOT NULL,

	UNIQUE (name)
);

CREATE TABLE identity (
	id			SERIAL PRIMARY KEY,
	address		TEXT NOT NULL,
	network_id	BIGINT NOT NULL,

	FOREIGN KEY (network_id)
		REFERENCES network (id),

	UNIQUE (address, network_id)
);

CREATE TABLE judgement (
	id					SERIAL PRIMARY KEY,
	identity_id			BIGINT NOT NULL,
	is_fully_verified	BOOLEAN NOT NULL DEFAULT FALSE,
	judgement_submitted BOOLEAN NOT NULL DEFAULT FALSE,
	inserted_timestamp	TIMESTAMP NOT NULL DEFAULT current_timestamp,
	completed_timestamp TIMESTAMP DEFAULT NULL,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE display_name (
	id			SERIAL PRIMARY KEY,
	identity_id BIGINT,
	network_id	BIGINT NOT NULL,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),
	FOREIGN KEY (network_id)
		REFERENCES network (id)
);

CREATE TABLE field_legal_name (
	id 					SERIAL PRIMARY KEY,
	identity_id			BIGINT NOT NULL,
	value				TEXT NOT NULL,
	manually_verified	BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE field_display_name (
	id 				SERIAL PRIMARY KEY,
	identity_id		BIGINT NOT NULL,
	value			TEXT NOT NULL,
	passed_check	BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE field_email (
	id 					SERIAL PRIMARY KEY,
	identity_id			BIGINT NOT NULL,
	value				TEXT NOT NULL,
	first_challenge		TEXT NOT NULL,
	second_challenge	TEXT NOT NULL,
	first_verified		BOOLEAN NOT NULL DEFAULT FALSE,
	second_verified		BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE field_web (
	id 					SERIAL PRIMARY KEY,
	identity_id			BIGINT NOT NULL,
	value				TEXT NOT NULL,
	manually_verified	BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE field_twitter (
	id 				SERIAL PRIMARY KEY,
	identity_id		BIGINT NOT NULL,
	value			TEXT NOT NULL,
	challenge		TEXT NOT NULL,
	is_verified		BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE field_matrix (
	id 				SERIAL PRIMARY KEY,
	identity_id		BIGINT NOT NULL,
	value			TEXT NOT NULL,
	challenge		TEXT NOT NULL,
	is_verified		BOOLEAN NOT NULL DEFAULT FALSE,

	FOREIGN KEY (identity_id)
		REFERENCES identity (id),

	UNIQUE (identity_id)
);

CREATE TABLE external_message_counter (
	id 			SERIAL PRIMARY KEY,
	adapter		TEXT NOT NULL,
	msg_id		BIGINT NOT NULL
);
