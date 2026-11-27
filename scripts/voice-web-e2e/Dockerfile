FROM ubuntu:24.04
RUN apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    pulseaudio pulseaudio-utils espeak-ng python3 python3-numpy curl xz-utils ca-certificates \
    libgtk-3-0t64 libasound2t64 libx11-xcb1 libdbus-glib-1-2 libxtst6 libpci3 libegl1 libgl1 \
    libnss3 libatk-bridge2.0-0t64 libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxrandr2 libgbm1 \
    libpango-1.0-0 libcairo2 libcups2t64 libxss1 fonts-dejavu-core >/dev/null && apt-get clean
ARG FIREFOX=161.0
RUN curl -fsSL "https://download-installer.cdn.mozilla.net/pub/firefox/releases/$FIREFOX/linux-x86_64/en-US/firefox-$FIREFOX.tar.xz" \
    | tar -xJ -C /opt
