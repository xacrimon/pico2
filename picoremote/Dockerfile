FROM debian:12.10

RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libusb-1.0-0-dev \
    cmake \
    git \
    openssh-server

WORKDIR /embedded

RUN git clone --depth=1 https://github.com/raspberrypi/pico-sdk
RUN cd pico-sdk && git submodule update --init lib/mbedtls

ENV PICO_SDK_PATH=/embedded/pico-sdk

RUN git clone --depth=1 https://github.com/raspberrypi/picotool
RUN cd picotool && mkdir build && cd build && cmake -DPICO_SDK_PATH=/embedded/pico-sdk/ ../ && make install

WORKDIR /embedded/picoremote

RUN mkdir /var/run/sshd
RUN echo "root:password" | chpasswd
RUN echo 'PermitRootLogin yes' >> /etc/ssh/sshd_config

CMD ["/usr/sbin/sshd", "-D"]
