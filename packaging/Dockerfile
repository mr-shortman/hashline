# Test the built artifact on a clean target, with no source/build-tree mounts.
FROM ubuntu:26.04
ENV DEBIAN_FRONTEND=noninteractive
# Ubuntu's minimal image excludes /usr/share/doc and manpages by default.
# Retain them here so dpkg -V verifies the complete installed payload.
RUN rm -f /etc/dpkg/dpkg.cfg.d/excludes \
    && apt-get update && apt-get install -y --no-install-recommends \
    dbus-x11 xvfb xauth python3-gi gir1.2-gtk-4.0 gir1.2-atspi-2.0 \
    at-spi2-core libglib2.0-bin fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
COPY target/packages/*.deb /packages/
COPY tests/desktop/native_reader.py /tests/native_reader.py
COPY packaging/smoke.sh /tests/smoke.sh
ENV XDG_CONFIG_HOME=/tmp/hashline-config \
    XDG_CACHE_HOME=/tmp/hashline-cache \
    XDG_DATA_HOME=/tmp/hashline-data \
    GTK_A11Y=atspi GDK_BACKEND=x11 GSK_RENDERER=cairo
CMD ["sh", "-c", "apt-get update && timeout 180 xvfb-run -a dbus-run-session -- sh /tests/smoke.sh /packages/hashline_*.deb"]
