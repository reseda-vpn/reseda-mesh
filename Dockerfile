FROM rust:1.66 as planner

WORKDIR /app

RUN cargo install cargo-chef 
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.66 as cacher

WORKDIR /app

RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1.66 as builder
WORKDIR /app
COPY . .

ARG db_key
ENV DATABASE_URL=$db_key

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

ARG mesh_key
ARG db_key
ARG cloudflare_key

ENV DATABASE_URL=$db_key

RUN echo "#!/bin/bash\n" \
         "  echo -e \"DATABASE_URL='$db_key'\nAUTHENTICATION_KEY='$mesh_key'\nCLOUDFLARE_KEY='$cloudflare_key'\" > ./app/.env\n"  > script.sh
RUN chmod +x script.sh
RUN ./script.sh

EXPOSE 8443/udp
EXPOSE 80
EXPOSE 443

WORKDIR /app

CMD ["./reseda-mesh"]
