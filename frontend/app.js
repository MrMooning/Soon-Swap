// OotleSwap frontend — live data from the public Esmeralda indexer.
// All AMM math is computed locally in BigInt; the user submits transactions
// via the Tari Wallet UI's manifest editor.

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
const MICRO = 1_000_000n;

// CBOR-style tag prefixes used by the indexer's substate JSON.
const TAG_RESOURCE = 131;
const TAG_VAULT = 132;

const $ = (sel) => document.querySelector(sel);

// ---- Substate helpers ----

async function fetchSubstate(addr) {
  const res = await fetch(`${INDEXER_URL}/substates/${addr}`);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${addr}`);
  return res.json();
}

function bytesToAddress(bytes, prefix) {
  return `${prefix}_${bytes.map(b => b.toString(16).padStart(2, "0")).join("")}`;
}

// In the indexer JSON, untagged pieces of state appear as
// { "Tag": [N, { "Bytes": [...] }] } where N tells you the entity type.
function parseTagged(node) {
  if (!node || typeof node !== "object") return null;
  if (!Array.isArray(node.Tag)) return null;
  const [tagNum, payload] = node.Tag;
  const bytes = payload?.Bytes;
  if (!Array.isArray(bytes)) return null;
  const prefix =
    tagNum === TAG_RESOURCE ? "resource" :
    tagNum === TAG_VAULT    ? "vault" :
    null;
  if (!prefix) return null;
  return bytesToAddress(bytes, prefix);
}

// u128 fields show as { "Array": [{ "Integer": lo }, { "Integer": hi }] }
function parseU128(node) {
  if (!node?.Array || node.Array.length !== 2) return 0n;
  const lo = BigInt(node.Array[0]?.Integer ?? 0);
  const hi = BigInt(node.Array[1]?.Integer ?? 0);
  return lo + (hi << 64n);
}

// Walk a "Map" node ([[{Text:k}, v], ...]) into a plain JS object.
function mapToObj(mapNode) {
  if (!Array.isArray(mapNode?.Map)) return {};
  const out = {};
  for (const [k, v] of mapNode.Map) {
    const key = k?.Text;
    if (key) out[key] = v;
  }
  return out;
}

// ---- Pool / vault read ----

async function readPoolState() {
  const poolSub = await fetchSubstate(ADDRS.POOL_COMPONENT);
  const state = mapToObj(poolSub.substate.Component.body.state);
  const vaultA = parseTagged(state.vault_a);
  const vaultB = parseTagged(state.vault_b);
  const lpResource = parseTagged(state.lp_resource);
  const lpTotalSupply = parseU128(state.lp_total_supply);

  if (!vaultA || !vaultB) {
    throw new Error("could not extract vault addresses from pool state");
  }

  // Resolve each vault to its current balance + resource.
  const [aSub, bSub] = await Promise.all([
    fetchSubstate(vaultA),
    fetchSubstate(vaultB),
  ]);
  const a = readVaultBalance(aSub);
  const b = readVaultBalance(bSub);

  return {
    reserveA: a.amount,
    reserveB: b.amount,
    resourceA: a.address,
    resourceB: b.address,
    lpResource,
    lpTotalSupply,
  };
}

// Vaults can hold Fungible (cleartext .amount) or Stealth (.revealed_amount,
// which is what the AMM operates on for tTARI). Confidential / NonFungible
// would need different handling.
function readVaultBalance(vaultSub) {
  const container = vaultSub?.substate?.Vault?.resource_container ?? {};
  if (container.Fungible) {
    return {
      amount: BigInt(container.Fungible.amount ?? 0),
      address: ensureResourcePrefix(container.Fungible.address),
    };
  }
  if (container.Stealth) {
    return {
      amount: BigInt(container.Stealth.revealed_amount ?? 0),
      address: ensureResourcePrefix(container.Stealth.address),
    };
  }
  throw new Error(
    `Unsupported vault resource_container shape: ${Object.keys(container).join(",") || "<empty>"}`
  );
}

function ensureResourcePrefix(addr) {
  return addr?.startsWith("resource_") ? addr : `resource_${addr}`;
}

async function readRegistry() {
  const sub = await fetchSubstate(ADDRS.FACTORY_COMPONENT);
  const state = mapToObj(sub.substate.Component.body.state);
  const poolsArr = state.pools?.Array ?? [];
  return poolsArr.map(entry => {
    const obj = mapToObj(entry);
    return {
      resource_a: parseTagged(obj.resource_a),
      resource_b: parseTagged(obj.resource_b),
      component: parseTagged(obj.component) ?? "(unparseable)",
    };
  });
}

// ---- AMM math ----

function quoteSwap(amountIn, reserveIn, reserveOut) {
  if (amountIn <= 0n || reserveIn <= 0n || reserveOut <= 0n) return 0n;
  const amountInWithFee = amountIn * FEE_NUM;
  const numerator = amountInWithFee * reserveOut;
  const denominator = reserveIn * FEE_DEN + amountInWithFee;
  return numerator / denominator;
}

function parseDecimalToMicro(s) {
  if (s === null || s === undefined || s === "") return 0n;
  const num = Number(s);
  if (!Number.isFinite(num) || num < 0) return 0n;
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

// Identify which reserve corresponds to SOON / tTARI by resource address.
function classifyReserves(state) {
  // resourceA/B are whatever the on-chain pool stored as vault_a / vault_b.
  // We tag them by which one matches our known TARI_TOKEN address.
  let reserveSoonMicro, reserveTariMicro;
  if (state.resourceA === ADDRS.TARI_TOKEN) {
    reserveTariMicro = state.reserveA;
    reserveSoonMicro = state.reserveB;
  } else {
    reserveSoonMicro = state.reserveA;
    reserveTariMicro = state.reserveB;
  }
  return { reserveSoonMicro, reserveTariMicro };
}

// ---- Render ----

function renderAddresses() {
  const container = $("#addresses");
  const rows = [
    ["POOL_COMPONENT",    ADDRS.POOL_COMPONENT],
    ["FACTORY_COMPONENT", ADDRS.FACTORY_COMPONENT],
    ["SOON_COMPONENT",    ADDRS.SOON_COMPONENT],
    ["SOON_RESOURCE",     ADDRS.SOON_RESOURCE],
    ["LP_RESOURCE",       ADDRS.LP_RESOURCE],
    ["POOL_TEMPLATE",     ADDRS.POOL_TEMPLATE],
    ["FACTORY_TEMPLATE",  ADDRS.FACTORY_TEMPLATE],
    ["SOON_TEMPLATE",     ADDRS.SOON_TEMPLATE],
  ];
  container.innerHTML = "";
  for (const [label, value] of rows) {
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

let currentReserves = null; // { reserveSoonMicro, reserveTariMicro, lpTotalSupply }

async function refreshLive() {
  const status = $("#fetch-status");
  status.textContent = "fetching live state…";
  status.style.color = "var(--muted)";
  try {
    const [poolState, registry] = await Promise.all([
      readPoolState(),
      readRegistry(),
    ]);
    const { reserveSoonMicro, reserveTariMicro } = classifyReserves(poolState);
    currentReserves = {
      reserveSoonMicro,
      reserveTariMicro,
      lpTotalSupply: poolState.lpTotalSupply,
    };

    $("#reserve-soon").value = formatMicroAsDecimal(reserveSoonMicro);
    $("#reserve-tari").value = formatMicroAsDecimal(reserveTariMicro);

    $("#lp-supply").textContent = `${formatMicroAsDecimal(poolState.lpTotalSupply)} LP`;
    $("#pool-count").textContent = `${registry.length}`;

    renderRegistry(registry);

    const now = new Date();
    status.textContent = `live · refreshed ${now.toLocaleTimeString()}`;
    status.style.color = "var(--success)";

    recompute();
  } catch (e) {
    console.error("live fetch failed:", e);
    status.textContent = `live fetch failed: ${e.message}. Enter values manually.`;
    status.style.color = "var(--error)";
  }
}

function renderRegistry(entries) {
  const container = $("#registry");
  if (!container) return;
  if (entries.length === 0) {
    container.innerHTML = `<p class="muted">No pools registered yet.</p>`;
    return;
  }
  container.innerHTML = entries.map((e, i) => `
    <div class="addr-row" data-component="${e.component}">
      <span class="label">#${i}</span>
      <span class="value">
        ${shortenAddr(e.resource_a)} ⇄ ${shortenAddr(e.resource_b)}
        <br><span class="muted">${e.component}</span>
      </span>
    </div>
  `).join("");
}

function shortenAddr(addr) {
  if (!addr) return "(?)";
  // resource_010101...01 is the native TARI; show it as TARI for readability.
  if (addr === ADDRS.TARI_TOKEN) return "tTARI";
  if (addr === ADDRS.SOON_RESOURCE) return "SOON";
  return addr.slice(0, 14) + "…" + addr.slice(-6);
}

function recompute() {
  const reserveSoonMicro = parseDecimalToMicro($("#reserve-soon").value);
  const reserveTariMicro = parseDecimalToMicro($("#reserve-tari").value);
  const amountInDec = $("#amount-in").value;
  const amountInMicro = parseDecimalToMicro(amountInDec);
  const side = $("#side").value;

  if (reserveSoonMicro > 0n && reserveTariMicro > 0n) {
    const ratio = Number(reserveSoonMicro) / Number(reserveTariMicro);
    $("#price-display").textContent = `1 tTARI ≈ ${ratio.toFixed(4)} SOON`;
  } else {
    $("#price-display").textContent = "—";
  }

  const outToken = side === "tari" ? "SOON" : "tTARI";
  $("#out-token").textContent = outToken;

  let amountOutMicro = 0n;
  let priceImpact = null;
  if (reserveSoonMicro > 0n && reserveTariMicro > 0n && amountInMicro > 0n) {
    const reserveIn  = side === "tari" ? reserveTariMicro : reserveSoonMicro;
    const reserveOut = side === "tari" ? reserveSoonMicro : reserveTariMicro;
    amountOutMicro = quoteSwap(amountInMicro, reserveIn, reserveOut);
    const spotOut = (amountInMicro * reserveOut) / reserveIn;
    if (spotOut > 0n) {
      const num = Number(spotOut - amountOutMicro);
      const den = Number(spotOut);
      priceImpact = (num / den) * 100;
    }
  }

  $("#amount-out").textContent = amountOutMicro > 0n
    ? formatMicroAsDecimal(amountOutMicro)
    : "—";
  $("#impact-display").textContent = priceImpact === null
    ? "—"
    : `${priceImpact.toFixed(2)}%`;

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

  $("#global-pool").querySelector("td:nth-child(2) code").textContent =
    ADDRS.POOL_COMPONENT;

  const soonRow = $("#global-soon");
  if (usesSoon) {
    soonRow.hidden = false;
    soonRow.querySelector("td:nth-child(2) code").textContent = ADDRS.SOON_RESOURCE;
  } else {
    soonRow.hidden = true;
  }
}

// ---- Wire up ----

// Generic helper for "copy contents of #sourceId, flash the button".
function bindCopyButton(buttonId, sourceId) {
  const btn = $(buttonId);
  if (!btn) return;
  btn.addEventListener("click", () => {
    const text = $(sourceId).textContent;
    navigator.clipboard.writeText(text).then(() => {
      const original = btn.textContent;
      btn.classList.add("copied");
      btn.textContent = "Copied ✓";
      setTimeout(() => {
        btn.classList.remove("copied");
        btn.textContent = original;
      }, 1200);
    });
  });
}

document.addEventListener("DOMContentLoaded", () => {
  renderAddresses();

  // Fill in the SOON component address in the faucet card's globals table.
  const faucetSoonComp = $("#faucet-soon-comp");
  if (faucetSoonComp) faucetSoonComp.textContent = ADDRS.SOON_COMPONENT;

  ["#reserve-soon", "#reserve-tari", "#amount-in", "#side"].forEach((sel) => {
    $(sel).addEventListener("input", recompute);
    $(sel).addEventListener("change", recompute);
  });

  $("#fetch-reserves").addEventListener("click", refreshLive);

  bindCopyButton("#copy-manifest", "#manifest");
  bindCopyButton("#copy-faucet", "#faucet-manifest");

  // Auto-fetch on load so the page comes up live.
  refreshLive();
});
