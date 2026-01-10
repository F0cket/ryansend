# Multi-stage Dockerfile for ryansend
FROM rust:1.92 AS builder

WORKDIR /usr/src/app

# Install cargo-about for license generation
RUN cargo install cargo-about

# Copy manifests and configuration
COPY Cargo.toml ./
COPY askama.toml ./
COPY about.hbs ./
COPY about.toml ./

# Copy source code and templates
COPY src ./src
COPY templates ./templates

# Build the application in release mode
RUN cargo build --release

# Generate license compliance file
RUN cargo about generate about.hbs > licenses.html

# Runtime stage - using Debian slim for glibc compatibility
FROM debian:bookworm-slim

# Install CA certificates and gosu for user switching
RUN apt-get update && \
    apt-get install -y ca-certificates gosu && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary, entrypoint script, and license files from builder stage
COPY --from=builder /usr/src/app/target/release/ryansend /usr/local/bin/ryansend
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
# Create dedicated directory for license files to avoid conflicts with user mounts
RUN mkdir -p /ryansend
COPY --from=builder /usr/src/app/licenses.html /ryansend/licenses.html
COPY LICENSE /ryansend/LICENSE

# Make binary and entrypoint executable and create working directory
RUN chmod +x /usr/local/bin/ryansend && \
    chmod +x /usr/local/bin/docker-entrypoint.sh && \
    mkdir -p /data
WORKDIR /data

# Expose the default port
EXPOSE 3000

# License and contribution files are available at:
# /ryansend/LICENSE - Project license (MIT)
# /ryansend/licenses.html - All dependency licenses


# Set entrypoint and default command
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["start"]
