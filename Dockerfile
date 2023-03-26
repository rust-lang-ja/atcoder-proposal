FROM ubuntu:22.10
RUN mkdir /judge
WORKDIR /judge
COPY ./install-command.bash /tmp/
RUN apt-get update
RUN apt-get install -y --no-install-recommends sudo
RUN bash /tmp/install-command.bash
