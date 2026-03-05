# Mixed Backend Runnable Example

This project demonstrates one module with:

- `shared`: backend-neutral shared definitions
- `server`: native HTTP server based on `moonbitlang/async`
- `frontend`: browser UI app based on `moonbit-community/rabbita`

## Run the server

From this directory:

```bash
moon run server --target native
```

The server listens on `http://127.0.0.1:3001` and serves:

- `GET /api/hello`

## Build and open the frontend

Build JS frontend:

```bash
moon build frontend --target js
```

Serve this directory statically, for example:

```bash
python3 -m http.server 8080
```

Then open:

- `http://127.0.0.1:8080/frontend/index.html`
