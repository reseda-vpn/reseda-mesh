FROM rust:1.61 as planner

WORKDIR /app

RUN cargo install cargo-chef 
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.61 as cacher

WORKDIR /app

RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1.61 as builder
WORKDIR /app
COPY . .

COPY --from=cacher /app/target target
RUN cargo build --release --bin reseda-mesh

FROM ubuntu:latest

RUN \
 mkdir /app \
 echo "**** install dependencies ****" && \
 apt-get update && \
 apt-get install -y --no-install-recommends \
    libc6 \
    sudo \
	bc \
	build-essential \
	curl \
	dkms \
	git \
	gnupg \ 
	ifupdown \
	iproute2 \
	iptables \
	iputils-ping \
	jq \
	libelf-dev \
	net-tools \
	openresolv \
	perl \
	pkg-config \
	qrencode \
	ca-certificates

COPY --from=builder /app/target/release/reseda-mesh ./app

COPY .env ./app
COPY cert.pem ./app
COPY key.pem ./app

EXPOSE 8443/udp
EXPOSE 80
EXPOSE 443

WORKDIR /app

CMD ["./reseda-mesh"]