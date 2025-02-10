FROM ubuntu:latest

RUN apt-get update && \
    apt-get install -y \
    build-essential \
    curl \
    git \
    libclang-dev \
    protobuf-compiler \
    unzip \
    && \
    rm -rf /var/cache/apt/archives /var/lib/apt/lists/*
