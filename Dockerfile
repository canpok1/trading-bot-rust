FROM rust:1.51 as builder
WORKDIR /usr/src/myapp
COPY . .
RUN cargo install --path . --bin bot

FROM rust:1.51-slim
COPY --from=builder /usr/local/cargo/bin/bot /usr/local/bin/bot
CMD ["bot"]
