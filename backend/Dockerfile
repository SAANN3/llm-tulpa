FROM rust:1-bookworm AS builder
WORKDIR /app

# libdrm-dev is needed at link time by gfxinfo's AMD-GPU-detection dependency
# (libdrm_amdgpu_sys) — not installed in the plain rust image by default.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libdrm-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY tool_derive ./tool_derive
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim
# Debian bookworm's own `nodejs` package is stuck on 18.20.4 (itself already EOL) —
# too old for current Playwright (needs Node >=20 as of late-2025 releases) and worth
# outgrowing regardless for a toolkit meant to stay current. Adds NodeSource's own apt
# repo so the real install line below just asks for `nodejs` and gets their current
# LTS instead; needs curl/ca-certificates/gnupg already present first, hence this
# tiny bootstrap ahead of everything else.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && rm -rf /var/lib/apt/lists/*

# libdrm2/libdrm-amdgpu1 are the runtime counterparts of the builder stage's
# libdrm-dev — linking needs the dev package, running needs the actual .so.
#
# Everything else on this line is a general-purpose toolkit for os.execute_command —
# baked into the image (permanent, survives every rebuild/restart) rather than left
# for the model to apt-get on demand (which only lives as long as the current
# container instance does — see the sudoers/pkg-mgmt comment below for *why* it can
# apt-get at all, and [[project_llm_tulpa_file_attachments]] for the "installs aren't
# persistent" finding that prompted baking this in properly instead). Deliberately a
# broad set of language runtimes/common CLI tools rather than trying to predict every
# specific library a future task might need — those are still installable at runtime
# (pip install/npm install/cargo add/go get all still work, just not permanently)
# on top of a real interpreter/compiler/package-manager foundation that's always
# there:
#   - python3 + pip + venv: the most common ad hoc scripting/calculator need (see
#     the "grinding through arithmetic by hand" SYSTEM_PROMPT rule this unblocks)
#   - nodejs (npm bundled, via NodeSource above), golang-go, rustc + cargo: the
#     other three languages likely to come up, each with their own package manager
#     already present
#   - build-essential: gcc/make, needed by anything that compiles C/C++ or a native
#     Python extension
#   - git: cloning/inspecting a repo
#   - poppler-utils: pdftotext, the exact real need that started this
#   - jq, unzip, zip, curl, wget: small, extremely common CLI utilities that cost
#     almost nothing to have around
#   - ripgrep, fd-find, tree, sqlite3: searching/exploring a codebase or directory
#     tree, and reading a .db file, all faster/more reliable than combinations of
#     storage.find_files + shell text processing for these specific jobs
#   - docx2txt, gnumeric: word/excel documents, the same class of need poppler-utils
#     covers for PDFs — docx2txt for a .docx's plain text, gnumeric's `ssconvert` CLI
#     to turn a .xlsx/.xls into .csv. Deliberately these two single-purpose tools
#     rather than pulling in all of LibreOffice for the same jobs.
#
# See `ExecuteCommandTool::description` (backend/src/tools/os/execute_command.rs) for
# this exact list mirrored into what the model itself is told is already available —
# keep the two in sync when this list changes.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 libdrm2 libdrm-amdgpu1 sudo \
        python3 python3-pip python3-venv \
        nodejs \
        golang-go \
        rustc cargo \
        build-essential \
        git jq unzip zip curl wget poppler-utils \
        ripgrep fd-find tree sqlite3 \
        docx2txt gnumeric \
    && rm -rf /var/lib/apt/lists/* \
    # Debian's fd-find package ships the binary as `fdfind`, not `fd` — a separate,
    # unrelated package already owns that name there. Symlinked so the model can just
    # run `fd` like anywhere else, instead of needing to already know this
    # distro-specific rename.
    && ln -s /usr/bin/fdfind /usr/local/bin/fd

# Playwright + a real headless Chromium, baked in the same permanent way as
# everything else above — for driving/screenshotting a real page (navigate, wait,
# screenshot to disk), then closing the loop with llm.read_image on the result.
# `--with-deps` pulls in the actual system libraries headless Chromium needs to
# launch at all (fonts, GTK, NSS, ...) — skipping it is what produces the classic
# "error while loading shared libraries" failure on a minimal image like this one.
#
# PLAYWRIGHT_BROWSERS_PATH: without this, the browser download below (run as root,
# during build) lands under /root/.cache/ms-playwright — unreadable by the non-root
# `appuser` this container actually runs as. Pointed somewhere world-readable instead,
# with the permissions to match, so launching it at runtime actually works.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
RUN npm install -g playwright \
    && npx playwright install --with-deps chromium \
    && chmod -R a+rX /opt/ms-playwright

# Lets the container's own (non-root, per compose.yaml's `user:`) process install
# packages it turns out to need at runtime — os.execute_command otherwise can't
# apt-get anything, since /var/lib/dpkg needs root. Deliberately scoped to just the
# package manager rather than full root: this container shares the host's PID
# namespace (`pid: host` in compose.yaml) with no user-namespace remap, so UID 0 in
# here is real UID 0 on the host, not sandboxed fake-root — full root would mean any
# host process is reachable, not just installing packages.
#
# `sudo` refuses to run at all for a UID with no `/etc/passwd` entry ("you do not
# exist in the passwd database") regardless of what the sudoers rule says — compose's
# `user: "${HOST_UID}:${HOST_GID}"` sets a bare numeric id with nothing backing it, so
# a real entry needs creating here too, not just the sudoers rule. `-M` (no home
# directory) since nothing here needs one; the shell is never actually used as a login
# shell (os.execute_command invokes `sh -c` directly), just set to something normal
# rather than `nologin` in case anything downstream ever inspects it.
ARG HOST_UID=1000
ARG HOST_GID=1000
RUN (groupadd -g "${HOST_GID}" appuser 2>/dev/null || true) \
    && useradd -u "${HOST_UID}" -g "${HOST_GID}" -M -s /bin/bash appuser \
    && echo "#${HOST_UID} ALL=(root) NOPASSWD: /usr/bin/apt-get, /usr/bin/apt, /usr/bin/dpkg" > /etc/sudoers.d/pkg-mgmt \
    && chmod 0440 /etc/sudoers.d/pkg-mgmt

COPY --from=builder /app/target/release/backend /usr/local/bin/backend

ENTRYPOINT ["/usr/local/bin/backend"]
