FROM ubuntu:16.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.cargo/bin:${PATH}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        coreutils \
        curl \
        git \
        software-properties-common \
        sudo \
        unzip \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf "https://sh.rustup.rs" | sh -s -- --default-toolchain none -y \
    && rustup toolchain install 1.90.0 --profile minimal --no-self-update \
    && rustup default 1.90.0

RUN curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" \
    && unzip awscliv2.zip \
    && bash ./aws/install \
    && rm -rf aws awscliv2.zip
