# BUILD STAGE
# Use the official Rust image as the base image for building
FROM rust:slim-trixie AS builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new directory for our application
WORKDIR /app

    # Copy over the Cargo.toml and Cargo.lock files
    COPY Cargo.toml Cargo.lock ./

# Create a dummy src/main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this will be cached unless Cargo.toml changes)
RUN cargo build --release && rm -rf src

# Copy the actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release

# FINAL STAGE
# Start a new stage for the runtime image
FROM debian:trixie-slim AS runner

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libpq5 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -r -s /bin/false stpame

# Create application directory
WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/stpa-me ./stpa-me

# Copy database schema if it exists
COPY database.sql ./database.sql

# Create directory for optional CSV file
RUN mkdir -p /app/data

# Change ownership of the application directory
RUN chown -R stpame:stpame /app

# Switch to non-root user
USER stpame

# Expose the port the app runs on (default 3000)
EXPOSE 3000

# Set environment variables for better error reporting
ENV RUST_BACKTRACE=1

# Run the binary
CMD ["./stpa-me"]
