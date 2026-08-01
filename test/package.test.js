import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const programPath = join(root, "solana", "programs", "mergeos-mrg", "src", "lib.rs");
const programSource = readFileSync(programPath, "utf8");
const readme = readFileSync(join(root, "README.md"), "utf8");
const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const idl = JSON.parse(readFileSync(join(root, "solana", "idl", "mergeos_mrg.json"), "utf8"));
const proof = JSON.parse(
  readFileSync(join(root, "solana", "mergeos_mrg.proof-manifest.v1.json"), "utf8"),
);
const deployments = JSON.parse(readFileSync(join(root, "deployments", "addresses.json"), "utf8"));

const PROGRAM_ID = "4gUBWum3fGKfm7BeGXryzXjPDBDLfhVJRcjN5MPnfDNW";

describe("mergeos-contracts package (Solana-first)", () => {
  it("exposes Solana Anchor workspace as the primary surface", () => {
    assert.equal(existsSync(join(root, "solana", "Anchor.toml")), true);
    assert.equal(existsSync(join(root, "solana", "Cargo.toml")), true);
    assert.equal(existsSync(programPath), true);
    assert.equal(existsSync(join(root, "solana", "idl", "mergeos_mrg.json")), true);
    assert.match(programSource, /use anchor_lang::prelude::\*/);
    assert.match(programSource, /use anchor_spl::token::\{/);
    assert.match(programSource, /#\[program\]\s*pub mod mergeos_mrg/);
    assert.match(programSource, new RegExp(`declare_id!\\("${PROGRAM_ID}"\\)`));
    assert.equal(idl.address, PROGRAM_ID);
    assert.equal(proof.program_id, PROGRAM_ID);
  });

  it("defines MRG mint, escrow, payout, treasury, and migration instructions", () => {
    for (const instruction of [
      "initialize_treasury",
      "mint_verified_mrg",
      "open_escrow",
      "release_payout",
      "register_legacy_wallet",
    ]) {
      assert.match(programSource, new RegExp(`pub fn ${instruction}\\(`));
    }
    assert.deepEqual(
      idl.instructions.map((item) => item.name),
      [
        "initializeTreasury",
        "mintVerifiedMrg",
        "openEscrow",
        "releasePayout",
        "registerLegacyWallet",
      ],
    );
  });

  it("anchors money events with ledger_reference bytes32", () => {
    for (const eventName of ["MRGMinted", "EscrowOpened", "PayoutReleased"]) {
      const block =
        programSource.match(new RegExp(`pub struct ${eventName} \\{([\\s\\S]*?)\\}`))?.[1] ||
        "";
      assert.match(block, /ledger_reference: \[u8; 32\]/, `${eventName} needs ledger_reference`);
    }
  });

  it("documents Solana env vars and deployment flow", () => {
    assert.match(readme, /MRG_SOLANA_PROGRAM_ID/);
    assert.match(readme, /CRYPTO_TOKEN_CONTRACT/);
    assert.match(readme, /Solana/i);
    assert.match(readme, /mergeos_mrg/);
    assert.equal(packageJson.name, "@mergeos/contracts");
    assert.equal(deployments.solana.localnet.program_id, PROGRAM_ID);
  });

  it("keeps TRON Solidity only under legacy/", () => {
    assert.equal(existsSync(join(root, "legacy", "tron", "MRGToken.sol")), true);
    assert.equal(existsSync(join(root, "contracts", "MRGToken.sol")), false);
    assert.match(readme, /[Ll]egacy/);
  });

  it("publishes token account security invariants on the IDL", () => {
    assert.ok(idl.metadata.security_invariants.length >= 6);
    for (const invariant of [
      "mint_verified_mrg requires receiver_token_account.mint == treasury_config.token_mint",
      "open_escrow requires escrow_token_account.mint == token_mint and escrow_token_account.owner == escrow_vault PDA",
      "release_payout requires worker_token_account.mint == treasury_config.token_mint",
    ]) {
      assert.ok(
        idl.metadata.security_invariants.includes(invariant),
        `missing invariant: ${invariant}`,
      );
    }
  });
});

describe("mergeos-contracts escrow close/withdraw audit (Issue #2)", () => {
  const CLOSE_WITHDRAW_INSTRUCTIONS = [
    "close_escrow",
    "withdraw_escrow",
    "refund_escrow",
    "cancel_escrow",
  ];

  it("documents that no close/withdraw/refund/cancel instruction exists in the program", () => {
    for (const instruction of CLOSE_WITHDRAW_INSTRUCTIONS) {
      const found = new RegExp(`pub fn ${instruction}\\(`).test(programSource);
      assert.equal(
        found,
        false,
        `MEDIUM finding (Issue #2): instruction '${instruction}' is MISSING — escrow funds can be permanently locked without a close/withdraw path. Expected: false (not yet implemented), got: ${found}`,
      );
    }
  });

  it("confirms the IDL lists no close/withdraw/refund/cancel instruction", () => {
    const idlInstructionNames = idl.instructions.map((item) => item.name);
    for (const idlName of [
      "closeEscrow",
      "withdrawEscrow",
      "refundEscrow",
      "cancelEscrow",
    ]) {
      assert.equal(
        idlInstructionNames.includes(idlName),
        false,
        `MEDIUM finding (Issue #2): IDL instruction '${idlName}' is MISSING. Expected: false (not yet in IDL), got: true`,
      );
    }
  });

  it("validates that the escrow vault PDA has no close authority set", () => {
    // Anchor close constraint: #[account(close = ...)] — verify none exists for escrow_vault
    const closeDirective = /#\[account\s*\([^)]*close\s*=\s*\w+/s;
    assert.equal(
      closeDirective.test(programSource),
      false,
      "MEDIUM finding (Issue #2): No #[account(close = ...)] directive found on any escrow_vault account — funds cannot be recovered",
    );
  });

  it("confirms existing instructions are intact (open_escrow and release_payout present)", () => {
    assert.match(programSource, /pub fn open_escrow\(/);
    assert.match(programSource, /pub fn release_payout\(/);
    // Sanity: the 5 core instructions are unchanged
    assert.deepEqual(
      idl.instructions.map((item) => item.name),
      [
        "initializeTreasury",
        "mintVerifiedMrg",
        "openEscrow",
        "releasePayout",
        "registerLegacyWallet",
      ],
      "IDL instruction list must match the 5 known instructions — no extras, no removals",
    );
  });
});
