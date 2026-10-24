# Voice input and https

The microphone button turns on once an owner has installed a speech model in Settings → Voice (the setup wizard offers it too): Parakeet for English, Russian and 23 other European languages, Omnilingual for about 1600 languages, Whisper large-v3 turbo for about 99. They are downloads of 300–560 MB, checked against a published hash, and run on the CPU.

A browser allows the microphone only on a secure page: `https://`, or `localhost`. On the machine itself `http://localhost:5173` already counts; from a phone or another computer use `https://<the machine's address>:5443`. The frontend container serves that port with a certificate it makes for itself on first start, so the browser warns about it once on each device: choose to continue anyway. The container makes that certificate for this machine's addresses when it starts, again when they have changed at the next start, and whenever you press "Make a new one" in Settings → Voice (use it after the machine's address changed, with the page open at the new address). To avoid the warning, put your own certificate in Settings → Voice (a PEM certificate and its key, without a password); the container serves it within a few seconds and goes back to its own when you remove it. The page on `5443` reaches the backend through the same address (the container forwards `/api`), so the backend has to be reachable from the container: `bind_addr` `0.0.0.0:3000`, as in the Docker example settings. `FRONTEND_HTTPS_PORT` in the root `.env` changes the port.

## Behind your own domain and reverse proxy
For a server on a domain, put your own reverse proxy in front, with a certificate your visitors' browsers trust (Let's Encrypt, say), and use `https://your.domain` on the normal port with no warning. The container's plain port `5173` serves the page and also forwards `/api` to the backend, so the proxy needs one rule: send everything to `5173`. With nginx:
```nginx
server {
    listen 443 ssl;
    server_name your.domain;
    ssl_certificate     /etc/letsencrypt/live/your.domain/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your.domain/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:5173;
        proxy_http_version 1.1;
        proxy_set_header Host $http_host;
        proxy_buffering off;      # the live event stream and replies as they are written
        proxy_read_timeout 1h;
        client_max_body_size 512m; # file attachments
    }
}
```
The page is then secure, so the microphone works, and the certificate settings of the container (port `5443`) are not used. Keep ports `3000` and `5443` closed to the internet: only the proxy needs to reach `5173`.
