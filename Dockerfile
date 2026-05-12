FROM ros:jazzy-ros-base AS base

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates build-essential pkg-config git \
      clang libclang-dev cmake python3-pip python3-colcon-common-extensions \
      ros-jazzy-rmw-cyclonedds-cpp ros-jazzy-std-msgs ros-jazzy-sensor-msgs \
      ros-jazzy-geometry-msgs ros-jazzy-nav-msgs ros-jazzy-diagnostic-msgs \
      ros-jazzy-tf2-msgs \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain 1.84.0 --profile minimal \
      --component rustfmt --component clippy

ENV RMW_IMPLEMENTATION=rmw_cyclonedds_cpp

WORKDIR /work

# Source ROS env in interactive shells
RUN echo "source /opt/ros/jazzy/setup.bash" >> /etc/bash.bashrc

CMD ["bash"]
