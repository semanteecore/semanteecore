FROM rust:latest as builder

# Install dependencies
RUN apt-get update && apt-get install -y cmake

# Install dependency builder for cargo
RUN cargo install cargo-build-deps

# That would be cached if Cargo.lock haven't been changed
RUN cd /tmp && USER=root cargo new --bin project
WORKDIR /tmp/project
COPY Cargo.toml Cargo.lock rust-toolchain ./
RUN cargo build-deps --release

# Copy the rest of sources & build
COPY src/ ./src
COPY grammar/ ./grammar
COPY README.md README.md
RUN cargo build --release
RUN mkdir /build -p && cp ./target/release/semanteecore /build/

# Use rust ubuntu base image for maximum flexibility of derived images
FROM rust:latest

ARG DOCKER_VERSION="18.09.6"
ENV DOWNLOAD_URL="https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_VERSION}.tgz"

# Set workdir for semanteecore
RUN mkdir /root/semantic
WORKDIR /root/semantic

# Install runtume dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    gcc \
    git \
    curl \
 && rm -rf /var/lib/apt/lists/*

# Install Docker client
RUN mkdir -p /tmp/download \
    && curl -L $DOWNLOAD_URL | tar -xz -C /tmp/download \
    && mv /tmp/download/docker/docker /usr/local/bin/ \
    && rm -rf /tmp/download

# Copy build binary
COPY --from=builder /build/semanteecore /usr/bin/

# Run cmd
CMD /bin/bash -c /usr/bin/semanteecore
