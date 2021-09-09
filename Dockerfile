from rust:slim

RUN mkdir /app 
RUN mkdir /app/bin 

COPY src /app/src/
COPY Cargo.toml /app

RUN apt-get update && apt-get install -y libssl-dev pkg-config
RUN cargo install --path /app --root /app

ENTRYPOINT ["/app/bin/avi-metrics-exporter"]

EXPOSE 8080
