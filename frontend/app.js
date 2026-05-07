// OotleSwap frontend — talks to the user, not the chain.
// All AMM math is computed locally; the user submits transactions via the
// Tari Wallet UI's manifest editor.

// ---- Live deployment (Esmeralda v0.2.0) ----
const ADDRS = {
  SOON_TEMPLATE:    "template_c57dd1a2529152fd20f9f75a62c15210db6ae101fb12c22882be138c3f625baa",
  POOL_TEMPLATE:    "template_317286e75618ff23f9ab6af0174bb08292fb04d9412033623c962824fcfe3cfa",
  FACTORY_TEMPLATE: "template_dc1be09374178bf80058808add496d4a1501772315fd37748c75d9822958cd75",
  SOON_COMPONENT:   "component_7c20414944194b905f9f63c73f479c80bf03627483276cf51f0f8a8c08a3b8fd",
  POOL_COMPONENT:   "component_3ab560338b91343b1a6ec1ccb21e47b23b3743ee72475603a0b0d1f41c147e40",
  FACTORY_COMPONENT:"component_13f0cd0752f8dc9ff3582efea511795da33e2480f2705568f2adb9614a74e222",
  SOON_RESOURCE:    "resource_7ca2d0f6b8b17000eb3b00d8d8c0e358c6ad097b9c4ba6b417823bcaead6062f",
  LP_RESOURCE:      "resource_3aaa0b1a17c896861fab1c3dc05de0dfc0173ed844338e935114038abfdb8db9",
  TARI_TOKEN:       "resource_0101010101010101010101010101010101010101010101010101010101010101",
};

const INDEXER_URL = "https://ootle-indexer-a.tari.com";
const FEE_NUM = 997n;
const FEE_DEN = 1000n;
const MICRO = 1_000_000n; // divisibility 6

// ---- DOM helpers ----
const $ = (sel) => document.querySelector(sel);

// ---- Render addresses with copy-on-click ----
function renderAddresses() {
  const container = $("#addresses");
  const rows = [
    ["POOL_COMPONENT",   ADDRS.POOL_COMPONENT,   "the AMM"],
    ["FACTORY_COMPONENT",ADDRS.FACTORY_COMPONENT,"registry"],
    ["SOON_COMPONENT",   ADDRS.SOON_COMPONENT,   "$SOON token + faucet"],
    ["SOON_RESOURCE",    ADDRS.SOON_RESOURCE,    ""],
    ["LP_RESOURCE",      ADDRS.LP_RESOURCE,      ""],
    ["POOL_TEMPLATE",    ADDRS.POOL_TEMPLATE,    ""],
    ["FACTORY_TEMPLATE", ADDRS.FACTORY_TEMPLATE, ""],
    ["SOON_TEMPLATE",    ADDRS.SOON_TEMPLATE,    ""],
  ];
  container.innerHTML = "";
  for (const [label, value, _hint] of rows) {
    const row = document.createElement("div");
    row.className = "addr-row";
    row.innerHTML = `
      <span class="label">${label}</span>
      <span class="value">${value}</span>
      <span class="copy-icon">⎘ copy</span>
    `;
    row.addEventListener("click", () => {
      navigator.clipboard.writeText(value).then(() => {
        row.classList.add("copied");
        row.querySelector(".copy-icon").textContent = "✓ copied";
        setTimeout(() => {
          row.classList.remove("copied");
          row.querySelector(".copy-icon").textContent = "⎘ copy";
        }, 1200);
      });
    });
    container.appendChild(row);
  }
}

// ---- Constant-product swap math (BigInt to match on-chain exactly) ----
// amount_out = floor((amount_in * 997 * reserve_out) / (reserve_in * 1000 + amount_in * 997))
function quoteSwap(amountIn, reserveIn, reserveOut) {
  if (amountIn <= 0n || reserveIn <= 0n || reserveOut <= 0n) return 0n;
  const amountInWithFee = amountIn * FEE_NUM;
  const numerator = amountInWithFee * reserveOut;
  const denominator = reserveIn * FEE_DEN + amountInWithFee;
  return numerator / denominator;
}

// Convert a decimal user input (e.g. "1.5") to micro-units BigInt.
function parseDecimalToMicro(s) {
  if (s === null || s === undefined || s === "") return 0n;
  const num = Number(s);
  if (!Number.isFinite(num) || num < 0) return 0n;
  // Round to 6 decimals to match divisibility.
  return BigInt(Math.round(num * 1_000_000));
}

function formatMicroAsDecimal(micro) {
  if (typeof micro !== "bigint") micro = BigInt(micro || 0);
  if (micro === 0n) return "0";
  const whole = micro / MICRO;
  const frac = micro % MICRO;
  const fracStr = frac.toString().padStart(6, "0").replace(/0+$/, "");
  return fracStr ? `${whole}.${fracStr}` : `${whole}`;
}

// ---- Live reserves fetch (best-effort) ----
async function tryFetchLiveReserves() {
  const status = $("#fetch-status");
  status.textContent = "fetching…";
  // We attempt the indexer's substate-get JSON-RPC. CORS may block;
  // we fail gracefully and ask the user to enter manually.
  try {
    const body = {
      jsonrpc: "2.0",
      id: 1,
      method: "get_substate",
      params: { address: ADDRS.POOL_COMPONENT },
    };
    const res = await fetch(INDEXER_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    // The shape of the response is engine-specific; we'd need to walk into
    // substate.component.body.state to find vault references and their
    // balances. Without specifying that here, we surface the response and
    // let the user check their wallet.
    console.log("indexer response:", data);
    status.textContent = "fetch succeeded — see browser console for raw response";
    status.style.color = "var(--success)";
  } catch (e) {
    console.warn("Live fetch failed (likely CORS):", e);
    status.textContent = "live fetch blocked (CORS) — enter values manually";
    status.style.color = "var(--error)";
  }
}

// ---- Rendering / interactivity ----
function recompute() {
  const reserveSoonMicro = parseDecimalToMicro($("#reserve-soon").value);
  const reserveTariMicro = parseDecimalToMicro($("#reserve-tari").value);
  const amountInDec = $("#amount-in").value;
  const amountInMicro = parseDecimalToMicro(amountInDec);
  const side = $("#side").value; // "tari" or "soon"

  // Spot price (no fee, no slippage): SOON per tTARI
  if (reserveSoonMicro > 0n && reserveTariMicro > 0n) {
    // 1 tTARI buys (reserve_soon / reserve_tari) SOON in the limit
    // Round to 4 decimal places for display.
    const ratio = Number(reserveSoonMicro) / Number(reserveTariMicro);
    $("#price-display").textContent = `1 tTARI ≈ ${ratio.toFixed(4)} SOON`;
  } else {
    $("#price-display").textContent = "—";
  }

  // Update output token label
  const outToken = side === "tari" ? "SOON" : "tTARI";
  $("#out-token").textContent = outToken;

  // Compute swap output
  let amountOutMicro = 0n;
  let priceImpact = null;
  if (reserveSoonMicro > 0n && reserveTariMicro > 0n && amountInMicro > 0n) {
    const reserveIn  = side === "tari" ? reserveTariMicro : reserveSoonMicro;
    const reserveOut = side === "tari" ? reserveSoonMicro : reserveTariMicro;
    amountOutMicro = quoteSwap(amountInMicro, reserveIn, reserveOut);

    // Price impact: how far from the spot price the user is getting?
    // spot_out = amount_in * reserve_out / reserve_in   (no fee, no slippage)
    // actual_out = amountOutMicro
    // impact = (spot_out - actual_out) / spot_out
    const spotOut = (amountInMicro * reserveOut) / reserveIn;
    if (spotOut > 0n) {
      const numerator = Number(spotOut - amountOutMicro);
      const denominator = Number(spotOut);
      priceImpact = (numerator / denominator) * 100;
    }
  }

  $("#amount-out").textContent = amountOutMicro > 0n
    ? formatMicroAsDecimal(amountOutMicro)
    : "—";
  $("#impact-display").textContent = priceImpact === null
    ? "—"
    : `${priceImpact.toFixed(2)}%`;

  // Build manifest
  const inResource = side === "tari" ? "TARI" : `var!["soon_resource"]`;
  const usesSoon = side === "soon";
  const amountMicroLiteral = amountInMicro.toString();

  let manifest;
  if (side === "tari") {
    manifest = `fn main() {
    let mut account = var!["account"];
    let pool = var!["pool"];

    let input = account.withdraw(TARI, Amount(${amountMicroLiteral}));
    let output = pool.swap(input);
    account.deposit(output);
}`;
  } else {
    manifest = `fn main() {
    let mut account = var!["account"];
    let pool = var!["pool"];
    let soon_resource = var!["soon_resource"];

    let input = account.withdraw(soon_resource, Amount(${amountMicroLiteral}));
    let output = pool.swap(input);
    account.deposit(output);
}`;
  }

  $("#manifest").textContent = manifest;

  // Update globals table
  $("#global-pool").querySelector("td:nth-child(2) code").textContent =
    ADDRS.POOL_COMPONENT;

  const soonRow = $("#global-soon");
  if (usesSoon) {
    soonRow.hidden = false;
    soonRow.querySelector("td:nth-child(2) code").textContent =
      ADDRS.SOON_RESOURCE;
  } else {
    soonRow.hidden = true;
  }
}

// ---- Wire up ----
document.addEventListener("DOMContentLoaded", () => {
  renderAddresses();

  // Pre-fill with the post-bootstrap reserves so calc starts useful.
  // (Will be off after subsequent swaps; user can refresh manually.)
  $("#reserve-soon").value = "100";
  $("#reserve-tari").value = "10";

  ["#reserve-soon", "#reserve-tari", "#amount-in", "#side"].forEach((sel) => {
    $(sel).addEventListener("input", recompute);
    $(sel).addEventListener("change", recompute);
  });

  $("#fetch-reserves").addEventListener("click", tryFetchLiveReserves);

  $("#copy-manifest").addEventListener("click", () => {
    const text = $("#manifest").textContent;
    navigator.clipboard.writeText(text).then(() => {
      const btn = $("#copy-manifest");
      btn.classList.add("copied");
      btn.textContent = "Copied ✓";
      setTimeout(() => {
        btn.classList.remove("copied");
        btn.textContent = "Copy";
      }, 1200);
    });
  });

  recompute();
});
