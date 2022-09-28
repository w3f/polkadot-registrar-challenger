# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.245.2/containers/rust/.devcontainer/base.Dockerfile

# [Choice] Debian OS version (use bullseye on local arm64/Apple Silicon): buster, bullseye
ARG VARIANT="buster"
FROM mcr.microsoft.com/vscode/devcontainers/rust:0-${VARIANT}

# [Optional] Uncomment this section to install additional packages.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
       lld pkg-config openssl libssl-dev gcc g++ clang cmake vim

RUN wget -qO - https://www.mongodb.org/static/pgp/server-4.4.asc | sudo apt-key add -
RUN echo "deb http://repo.mongodb.org/apt/debian buster/mongodb-org/4.4 main" | sudo tee /etc/apt/sources.list.d/mongodb-org-4.4.list
RUN sudo apt-get update
RUN sudo apt-get install -y mongodb-org

RUN mkdir -p /data/db
RUN chown vscode:vscode -R /data/db


RUN echo "\
replication:\n\
    replSetName: \"rs0\"\n\
setParameter:\n\
    maxTransactionLockRequestTimeoutMillis: 10000\n\
" >> /etc/mongod.conf
