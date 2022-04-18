#!/bin/bash
cd "$(dirname "$0")"

# Prevent `psql` from prompting
export PGPASSWORD=$POSTGRES_PASSWORD

psql -U ${POSTGRES_USER} \
	-h ${POSTGRES_HOSTNAME} \
	-p ${POSTGRES_PORT} \
	-c "DROP DATABASE ${POSTGRES_DB} WITH (FORCE)"
psql -U ${POSTGRES_USER} \
	-h ${POSTGRES_HOSTNAME} \
	-p ${POSTGRES_PORT} \
	-c "CREATE DATABASE ${POSTGRES_DB}"

psql -U ${POSTGRES_USER} \
	-h ${POSTGRES_HOSTNAME} \
	-p ${POSTGRES_PORT} \
	-d ${POSTGRES_DB} \
	-a -f init.sql