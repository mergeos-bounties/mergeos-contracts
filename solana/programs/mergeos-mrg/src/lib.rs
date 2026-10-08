use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("FrSanGwt8MjCqnfYRBiHFdtygqrL41uxTZin2AR71dRY");

const MAX_SYMBOL_LEN: usize = 16;

#[program]
pub mod mergeos_mrg {
    use super::*;

    pub fn initialize_treasury(
        ctx: Context<InitializeTreasury>,
        token_symbol: String,
    ) -> Result<()> {
        require!(!token_symbol.trim().is_empty(), MergeOSError::InvalidTokenSymbol); // checks to see if its empty
        require!(token_symbol.len() <= MAX_SYMBOL_LEN, MergeOSError::InvalidTokenSymbol); // checks to see if its less than the max

        let config = &mut ctx.accounts.treasury_config;
        config.authority = ctx.accounts.authority.key(); // updates seem okay
        config.token_mint = ctx.accounts.token_mint.key(); // updates seem okay
        config.treasury_token_account = ctx.accounts.treasury_token_account.key(); // updates seem okay
        config.token_symbol = token_symbol; // updates seem okay
        config.bump = ctx.bumps.treasury_config; // updates seem okay
        config.created_at = Clock::get()?.unix_timestamp; // updates seem okay
        config.minted_amount = 0; // updates seem okay
        config.escrowed_amount = 0; // updates seem okay
        config.released_amount = 0; // updates seem okay
        Ok(())
    }

    pub fn mint_verified_mrg(
        ctx: Context<MintVerifiedMRG>,
        ledger_reference: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, MergeOSError::InvalidAmount); // makes sense
        // @audit-info can this be added as a anchor constraint on the MintVerifiedMRG account struct
        require_keys_eq!(
            ctx.accounts.treasury_config.authority,
            ctx.accounts.authority.key(),
            MergeOSError::Unauthorized
        );

        // CPI seems okay (Token Program => MintTO ix to receiver_token_account)
        let signer_seeds: &[&[u8]] = &[b"treasury", &[ctx.accounts.treasury_config.bump]];
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    to: ctx.accounts.receiver_token_account.to_account_info(),
                    authority: ctx.accounts.treasury_config.to_account_info(),
                },
                &[signer_seeds],
            ),
            amount,
        )?;

        let config = &mut ctx.accounts.treasury_config;
        config.minted_amount = config
            .minted_amount
            .checked_add(amount)
            .ok_or(MergeOSError::MathOverflow)?; // update correct
        emit!(MRGMinted {
            ledger_reference,
            amount,
            receiver: ctx.accounts.receiver_token_account.key(),
        }); // event emitted correctly
        Ok(())
    }

    pub fn open_escrow(
        ctx: Context<OpenEscrow>,
        project_id: [u8; 32],
        ledger_reference: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, MergeOSError::InvalidAmount); // this check is valid

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.funder_token_account.to_account_info(),
                    to: ctx.accounts.escrow_token_account.to_account_info(),
                    authority: ctx.accounts.funder.to_account_info(),
                },
            ),
            amount,
        )?; // CPI is okay

        let escrow = &mut ctx.accounts.escrow_vault;
        escrow.project_id = project_id;
        escrow.funder = ctx.accounts.funder.key();
        escrow.token_mint = ctx.accounts.token_mint.key();
        escrow.escrow_token_account = ctx.accounts.escrow_token_account.key();
        escrow.opened_ledger_reference = ledger_reference;
        escrow.opened_amount = escrow
            .opened_amount
            .checked_add(amount)
            .ok_or(MergeOSError::MathOverflow)?;
        escrow.remaining_amount = escrow
            .remaining_amount
            .checked_add(amount)
            .ok_or(MergeOSError::MathOverflow)?;
        escrow.bump = ctx.bumps.escrow_vault;
        escrow.updated_at = Clock::get()?.unix_timestamp;

        let config = &mut ctx.accounts.treasury_config;
        config.escrowed_amount = config
            .escrowed_amount
            .checked_add(amount)
            .ok_or(MergeOSError::MathOverflow)?;
        emit!(EscrowOpened {
            project_id,
            ledger_reference,
            amount,
            funder: ctx.accounts.funder.key(),
        });
        Ok(())
    }

    pub fn release_payout(
        ctx: Context<ReleasePayout>,
        payout_id: [u8; 32],
        ledger_reference: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, MergeOSError::InvalidAmount);
        require_keys_eq!(
            ctx.accounts.treasury_config.authority,
            ctx.accounts.authority.key(),
            MergeOSError::Unauthorized
        ); // check makes a lot of sense
        require!(
            ctx.accounts.escrow_vault.remaining_amount >= amount,
            MergeOSError::InsufficientEscrow
        ); // makes sense

        let escrow = &mut ctx.accounts.escrow_vault;
        let signer_seeds: &[&[u8]] = &[b"escrow", escrow.project_id.as_ref(), &[escrow.bump]];
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.escrow_token_account.to_account_info(),
                    to: ctx.accounts.worker_token_account.to_account_info(),
                    authority: escrow.to_account_info(),
                },
                &[signer_seeds],
            ),
            amount,
        )?; // makes sense

        escrow.remaining_amount = escrow
            .remaining_amount
            .checked_sub(amount)
            .ok_or(MergeOSError::MathOverflow)?; // makes sense
        escrow.updated_at = Clock::get()?.unix_timestamp; // makes sense

        let receipt = &mut ctx.accounts.payout_receipt;
        receipt.payout_id = payout_id; // makes sense
        receipt.project_id = escrow.project_id; // makes sense
        receipt.worker = ctx.accounts.worker.key(); // makes sense
        receipt.worker_token_account = ctx.accounts.worker_token_account.key(); // makes sense
        receipt.ledger_reference = ledger_reference; // makes sense
        receipt.amount = amount; // makes sense
        receipt.bump = ctx.bumps.payout_receipt; // makes sense
        receipt.released_at = Clock::get()?.unix_timestamp; // makes sense

        let config = &mut ctx.accounts.treasury_config;
        config.released_amount = config
            .released_amount
            .checked_add(amount)
            .ok_or(MergeOSError::MathOverflow)?; // makes sense

        // FIX: keep escrowed_amount in sync with funds leaving escrow
        config.escrowed_amount = config
            .escrowed_amount
            .checked_sub(amount)
            .ok_or(MergeOSError::MathOverflow)?;
        emit!(PayoutReleased {
            payout_id,
            project_id: escrow.project_id,
            ledger_reference,
            amount,
            worker: ctx.accounts.worker.key(),
        });
        Ok(())
    }

    pub fn register_legacy_wallet(
        ctx: Context<RegisterLegacyWallet>,
        legacy_chain: LegacyChain,
        legacy_address_hash: [u8; 32],
        solana_wallet: Pubkey,
    ) -> Result<()> {
        require_keys_eq!(
            solana_wallet,
            ctx.accounts.solana_wallet.key(),
            MergeOSError::WalletMismatch
        ); // check has been made

        let migration = &mut ctx.accounts.wallet_migration;
        migration.legacy_chain = legacy_chain;
        migration.legacy_address_hash = legacy_address_hash;
        migration.solana_wallet = solana_wallet;
        migration.owner = ctx.accounts.owner.key();
        migration.registered_at = Clock::get()?.unix_timestamp;
        migration.bump = ctx.bumps.wallet_migration;
        emit!(LegacyWalletRegistered {
            legacy_chain,
            legacy_address_hash,
            solana_wallet,
            owner: ctx.accounts.owner.key(),
        }); // everything is correct
        Ok(())
    }
}

pub fn close_escrow(ctx: Context<CloseEscrow>) -> Result<()> {
    // Both conditions as requested
    require!(
        ctx.accounts.escrow_vault.remaining_amount == 0,
        MergeOSError::EscrowNotEmpty
    );
    require!(
        ctx.accounts.treasury_config.escrowed_amount == 0,
        MergeOSError::EscrowNotEmpty
    );
    // The real token balance must be zero too, or close_account will fail
    require!(
        ctx.accounts.escrow_token_account.amount == 0,
        MergeOSError::EscrowNotEmpty
    );

    let project_id = ctx.accounts.escrow_vault.project_id;
    let bump = ctx.accounts.escrow_vault.bump;
    let signer_seeds: &[&[u8]] = &[b"escrow", project_id.as_ref(), &[bump]];

    // Close the escrow token account, rent goes back to the funder
    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.escrow_token_account.to_account_info(),
            destination: ctx.accounts.funder.to_account_info(),
            authority: ctx.accounts.escrow_vault.to_account_info(),
        },
        &[signer_seeds],
    ))?;

    emit!(EscrowClosed {
        project_id,
        funder: ctx.accounts.funder.key(),
    });
    // escrow_vault itself is closed by the `close = funder` constraint
    Ok(())
}

#[derive(Accounts)]
#[instruction(token_symbol: String)]
pub struct InitializeTreasury<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_mint: Account<'info, Mint>,
    pub treasury_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = authority,
        space = 8 + TreasuryConfig::INIT_SPACE,
        seeds = [b"treasury"],
        bump
    )]
    pub treasury_config: Account<'info, TreasuryConfig>, // rent is paid by the authority pubkey
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintVerifiedMRG<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut, seeds = [b"treasury"], bump = treasury_config.bump)]
    pub treasury_config: Account<'info, TreasuryConfig>,
    #[account(mut, address = treasury_config.token_mint)]
    pub token_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = receiver_token_account.mint == token_mint.key() @ MergeOSError::TokenMintMismatch
    )]
    pub receiver_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(project_id: [u8; 32])]
pub struct OpenEscrow<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,
    #[account(mut, seeds = [b"treasury"], bump = treasury_config.bump)]
    pub treasury_config: Account<'info, TreasuryConfig>,
    #[account(address = treasury_config.token_mint)]
    pub token_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = funder_token_account.mint == token_mint.key() @ MergeOSError::TokenMintMismatch,
        constraint = funder_token_account.owner == funder.key() @ MergeOSError::TokenAuthorityMismatch
    )]
    pub funder_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = funder,
        space = 8 + EscrowVault::INIT_SPACE,
        seeds = [b"escrow", project_id.as_ref()],
        bump
    )]
    pub escrow_vault: Account<'info, EscrowVault>,
    #[account(
        mut,
        constraint = escrow_token_account.mint == token_mint.key() @ MergeOSError::TokenMintMismatch,
        constraint = escrow_token_account.owner == escrow_vault.key() @ MergeOSError::TokenAuthorityMismatch
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(payout_id: [u8; 32])]
pub struct ReleasePayout<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut, seeds = [b"treasury"], bump = treasury_config.bump)]
    pub treasury_config: Account<'info, TreasuryConfig>,
    #[account(mut, seeds = [b"escrow", escrow_vault.project_id.as_ref()], bump = escrow_vault.bump)]
    pub escrow_vault: Account<'info, EscrowVault>,
    #[account(
        mut,
        address = escrow_vault.escrow_token_account,
        constraint = escrow_token_account.mint == treasury_config.token_mint @ MergeOSError::TokenMintMismatch,
        constraint = escrow_token_account.owner == escrow_vault.key() @ MergeOSError::TokenAuthorityMismatch
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    /// CHECK: Worker identity is stored for proof and can be a wallet owner PDA later.
    pub worker: AccountInfo<'info>,
    #[account(
        mut,
        constraint = worker_token_account.mint == treasury_config.token_mint @ MergeOSError::TokenMintMismatch
    )]
    pub worker_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = authority,
        space = 8 + PayoutReceipt::INIT_SPACE,
        seeds = [b"payout", payout_id.as_ref()],
        bump
    )]
    pub payout_receipt: Account<'info, PayoutReceipt>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(legacy_chain: LegacyChain, legacy_address_hash: [u8; 32])]
pub struct RegisterLegacyWallet<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    /// CHECK: The registered target wallet must match the `solana_wallet` argument.
    pub solana_wallet: AccountInfo<'info>,
    #[account(
        init,
        payer = owner,
        space = 8 + WalletMigration::INIT_SPACE,
        seeds = [
            b"wallet-migration",
            legacy_chain.seed().as_bytes(),
            legacy_address_hash.as_ref()
        ],
        bump
    )]
    pub wallet_migration: Account<'info, WalletMigration>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseEscrow<'info> {
    pub authority: Signer<'info>,
    #[account(
        seeds = [b"treasury"],
        bump = treasury_config.bump,
        has_one = authority @ MergeOSError::Unauthorized
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,
    #[account(
        mut,
        seeds = [b"escrow", escrow_vault.project_id.as_ref()],
        bump = escrow_vault.bump,
        close = funder
    )]
    pub escrow_vault: Account<'info, EscrowVault>,
    #[account(
        mut,
        address = escrow_vault.escrow_token_account,
        constraint = escrow_token_account.owner == escrow_vault.key() @ MergeOSError::TokenAuthorityMismatch
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    /// CHECK: must be the original funder, receives the rent refunds
    #[account(mut, address = escrow_vault.funder)]
    pub funder: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
#[derive(InitSpace)]
pub struct TreasuryConfig {
    pub authority: Pubkey,
    pub token_mint: Pubkey,
    pub treasury_token_account: Pubkey,
    #[max_len(MAX_SYMBOL_LEN)]
    pub token_symbol: String,
    pub bump: u8,
    pub created_at: i64,
    pub minted_amount: u64,
    pub escrowed_amount: u64,
    pub released_amount: u64,
}

#[account]
#[derive(InitSpace)]
pub struct EscrowVault {
    pub project_id: [u8; 32],
    pub funder: Pubkey,
    pub token_mint: Pubkey,
    pub escrow_token_account: Pubkey,
    pub opened_ledger_reference: [u8; 32],
    pub opened_amount: u64,
    pub remaining_amount: u64,
    pub bump: u8,
    pub updated_at: i64,
}

#[account]
#[derive(InitSpace)]
pub struct PayoutReceipt {
    pub payout_id: [u8; 32],
    pub project_id: [u8; 32],
    pub worker: Pubkey,
    pub worker_token_account: Pubkey,
    pub ledger_reference: [u8; 32],
    pub amount: u64,
    pub bump: u8,
    pub released_at: i64,
}

#[account]
#[derive(InitSpace)]
pub struct WalletMigration {
    pub legacy_chain: LegacyChain,
    pub legacy_address_hash: [u8; 32],
    pub solana_wallet: Pubkey,
    pub owner: Pubkey,
    pub bump: u8,
    pub registered_at: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum LegacyChain {
    Trc20,
    Evm,
}

impl LegacyChain {
    pub fn seed(&self) -> &'static str {
        match self {
            LegacyChain::Trc20 => "trc20",
            LegacyChain::Evm => "evm",
        }
    }
}

#[event]
pub struct MRGMinted {
    pub ledger_reference: [u8; 32],
    pub amount: u64,
    pub receiver: Pubkey,
}

#[event]
pub struct EscrowOpened {
    pub project_id: [u8; 32],
    pub ledger_reference: [u8; 32],
    pub amount: u64,
    pub funder: Pubkey,
}

#[event]
pub struct PayoutReleased {
    pub payout_id: [u8; 32],
    pub project_id: [u8; 32],
    pub ledger_reference: [u8; 32],
    pub amount: u64,
    pub worker: Pubkey,
}

#[event]
pub struct LegacyWalletRegistered {
    pub legacy_chain: LegacyChain,
    pub legacy_address_hash: [u8; 32],
    pub solana_wallet: Pubkey,
    pub owner: Pubkey,
}

#[event]
pub struct EscrowClosed {
    pub project_id: [u8; 32],
    pub funder: Pubkey,
}

#[error_code]
pub enum MergeOSError {
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Token symbol is empty or too long")]
    InvalidTokenSymbol,
    #[msg("The signer is not authorized for this treasury action")]
    Unauthorized,
    #[msg("Escrow balance is not sufficient for this payout")]
    InsufficientEscrow,
    #[msg("The Solana wallet account does not match the instruction argument")]
    WalletMismatch,
    #[msg("Token account mint does not match the MRG mint")]
    TokenMintMismatch,
    #[msg("Token account authority does not match the expected PDA or signer")]
    TokenAuthorityMismatch,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Escrow still holds funds and cannot be closed")]
    EscrowNotEmpty,
}
