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

# Install CA certificates and create app user
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -m -u 1000 appuser

# Copy the binary and license files from builder stage
COPY --from=builder /usr/src/app/target/release/ryansend /usr/local/bin/ryansend
# Create dedicated directory for license files to avoid conflicts with user mounts
RUN mkdir -p /ryansend
COPY --from=builder /usr/src/app/licenses.html /ryansend/licenses.html
COPY LICENSE /ryansend/LICENSE
COPY DCO /ryansend/DCO

# Make binary executable and create working directory
RUN chmod +x /usr/local/bin/ryansend && \
    mkdir -p /app && \
    chown appuser:appuser /app

# Switch to non-root user
USER appuser
WORKDIR /app

# Expose the default port
EXPOSE 3000

# License and contribution files are available at:
# /ryansend/LICENSE - Project license (MIT)
# /ryansend/licenses.html - All dependency licenses
# /ryansend/DCO - Developer Certificate of Origin
# /ryansend/CONTRIBUTING.md - Contribution guidelines

# Default command - will auto-init if no config exists
CMD ["ryansend", "start"]
