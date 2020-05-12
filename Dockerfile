FROM rustlang/rust:nightly

RUN apt-get update

RUN mkdir /usr/local/raistlin
WORKDIR /usr/local/raistlin
COPY . .
RUN cargo run
