# OotleSwap frontend

A static single-page app for the OotleSwap pool. Three files, no build step:

- `index.html` — markup
- `style.css`  — dark themed, ~5KB
- `app.js`     — vanilla JS, BigInt-based AMM math (matches on-chain to within 1 micro-unit)

## What it does

- Lists the live deployment addresses (click to copy)
- Pool reserves card (manual entry; live fetch attempted but blocked by CORS in most browsers)
- Swap calculator — local x*y=k math with 0.3% fee, live preview
- Generates the wallet-UI manifest text for the swap, with the right globals listed

It does **not** sign or submit transactions. The user takes the generated manifest
into their Tari wallet UI's Manifest editor and submits there. This avoids needing
browser ↔ wallet daemon RPC plumbing while still being a useful interactive surface.

## Run locally

Any static server works:

```bash
python3 -m http.server 8000 --directory frontend
# or
npx serve frontend
```

Then open http://localhost:8000.

## Deployment

GitHub Actions workflow at `.github/workflows/pages.yml` deploys this directory
to GitHub Pages on every push to `main` that touches `frontend/`.

After the first run, find the live URL in the Actions tab → "Deploy frontend to
GitHub Pages" run → "Deploy to GitHub Pages" step output. Typically:

```
https://MrMooning.github.io/Soon-Swap/
```
