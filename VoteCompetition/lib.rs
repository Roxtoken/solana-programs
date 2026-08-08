
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("VoteVault11111111111111111111111111111111111");

#[program]
pub mod vote_competition {
    use super::*;

    /// Initialize the global vote vault state
    pub fn initialize_vault(ctx: Context<InitializeVault>, refund_duration_seconds: i64) -> Result<()> {
        let vault = &mut ctx.accounts.vault_state;
        vault.master = ctx.accounts.master.key();
        vault.mint = ctx.accounts.mint.key();
        vault.total_vote_amount = 0;
        vault.is_refund_enabled = false;
        vault.refund_duration = refund_duration_seconds;
        vault.refund_start_time = 0;
        vault.bump = ctx.bumps.vault_state;
        vault.token_vault_bump = ctx.bumps.token_vault;
        Ok(())
    }

    /// Register or track a referrer on-chain using their unique referral code
    pub fn register_referrer(ctx: Context<RegisterReferrer>, ref_code: String) -> Result<()> {
        require!(ref_code.len() <= 8, VoteError::InvalidRefCode);
        let referrer_state = &mut ctx.accounts.referrer_state;
        referrer_state.referrer = ctx.accounts.referrer.key();
        referrer_state.ref_code = ref_code;
        referrer_state.total_referred_votes = 0;
        referrer_state.bump = ctx.bumps.referrer_state;
        Ok(())
    }

    /// Cast a vote: Transfers SPL tokens into vault and records voter + referrer stats
    pub fn vote(ctx: Context<Vote>, amount: u64) -> Result<()> {
        let vault = &ctx.accounts.vault_state;
        require!(!vault.is_refund_enabled, VoteError::VotingClosed);
        require!(amount > 0, VoteError::InvalidAmount);

        // Transfer tokens from voter to PDA Token Vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.voter_token_account.to_account_info(),
            to: ctx.accounts.token_vault.to_account_info(),
            authority: ctx.accounts.voter.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update Global Vault State
        let vault = &mut ctx.accounts.vault_state;
        vault.total_vote_amount = vault.total_vote_amount.checked_add(amount).unwrap();

        // Update Referrer Stats
        let referrer = &mut ctx.accounts.referrer_state;
        referrer.total_referred_votes = referrer.total_referred_votes.checked_add(amount).unwrap();

        // Update Voter Record
        let voter_record = &mut ctx.accounts.voter_record;
        if voter_record.voter == Pubkey::default() {
            voter_record.voter = ctx.accounts.voter.key();
            voter_record.bump = ctx.bumps.voter_record;
        }
        voter_record.contribution_amount = voter_record.contribution_amount.checked_add(amount).unwrap();

        Ok(())
    }

    /// Master enables or disables refund mode
    pub fn set_refund_state(ctx: Context<SetRefundState>, enable_refund: bool) -> Result<()> {
        let vault = &mut ctx.accounts.vault_state;
        vault.is_refund_enabled = enable_refund;
        if enable_refund {
            vault.refund_start_time = Clock::get()?.unix_timestamp;
        }
        Ok(())
    }

    /// Voters claim their contributed tokens back if master set Refund = True
    pub fn claim_refund(ctx: Context<ClaimRefund>) -> Result<()> {
        let vault = &ctx.accounts.vault_state;
        require!(vault.is_refund_enabled, VoteError::RefundNotActive);

        let voter_record = &mut ctx.accounts.voter_record;
        let refund_amount = voter_record.contribution_amount;
        require!(refund_amount > 0, VoteError::NoContributionToRefund);

        voter_record.contribution_amount = 0;

        // Transfer tokens back to voter using PDA Signer
        let seeds = &[b"vault_state".as_ref(), &[vault.bump]];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.token_vault.to_account_info(),
            to: ctx.accounts.voter_token_account.to_account_info(),
            authority: ctx.accounts.vault_state.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, refund_amount)?;

        Ok(())
    }

    /// Master sweeps remaining funds and closes the vault after RefundTime has elapsed
    pub fn close_and_sweep_vault(ctx: Context<SweepVault>) -> Result<()> {
        let vault = &ctx.accounts.vault_state;
        let current_time = Clock::get()?.unix_timestamp;

        if vault.is_refund_enabled {
            require!(
                current_time >= vault.refund_start_time.checked_add(vault.refund_duration).unwrap(),
                VoteError::RefundWindowStillActive
            );
        }

        let remaining_tokens = ctx.accounts.token_vault.amount;
        let seeds = &[b"vault_state".as_ref(), &[vault.bump]];
        let signer_seeds = &[&seeds[..]];

        // 1. Sweep remaining tokens to Master
        if remaining_tokens > 0 {
            let cpi_accounts = Transfer {
                from: ctx.accounts.token_vault.to_account_info(),
                to: ctx.accounts.master_token_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, remaining_tokens)?;
        }

        // 2. Close SPL token vault account (returns rent to master)
        let close_accounts = token::CloseAccount {
            account: ctx.accounts.token_vault.to_account_info(),
            destination: ctx.accounts.master.to_account_info(),
            authority: ctx.accounts.vault_state.to_account_info(),
        };
        let close_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            close_accounts,
            signer_seeds,
        );
        token::close_account(close_ctx)?;

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Context Accounts
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(
        init,
        payer = master,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [b"vault_state"],
        bump
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        init,
        payer = master,
        seeds = [b"token_vault", vault_state.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = vault_state,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub master: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(ref_code: String)]
pub struct RegisterReferrer<'info> {
    #[account(
        init_if_needed,
        payer = referrer,
        space = 8 + ReferrerState::INIT_SPACE,
        seeds = [b"referrer", ref_code.as_bytes()],
        bump
    )]
    pub referrer_state: Account<'info, ReferrerState>,
    #[account(mut)]
    pub referrer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Vote<'info> {
    #[account(mut, seeds = [b"vault_state"], bump = vault_state.bump)]
    pub vault_state: Account<'info, VaultState>,

    #[account(mut, seeds = [b"token_vault", vault_state.key().as_ref()], bump = vault_state.token_vault_bump)]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub referrer_state: Account<'info, ReferrerState>,

    #[account(
        init_if_needed,
        payer = voter,
        space = 8 + VoterRecord::INIT_SPACE,
        seeds = [b"voter_record", voter.key().as_ref()],
        bump
    )]
    pub voter_record: Account<'info, VoterRecord>,

    #[account(mut)]
    pub voter_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub voter: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetRefundState<'info> {
    #[account(
        mut,
        seeds = [b"vault_state"],
        bump = vault_state.bump,
        has_one = master @ VoteError::Unauthorized
    )]
    pub vault_state: Account<'info, VaultState>,
    pub master: Signer<'info>,
}

#[derive(Accounts)]
pub struct ClaimRefund<'info> {
    #[account(seeds = [b"vault_state"], bump = vault_state.bump)]
    pub vault_state: Account<'info, VaultState>,

    #[account(mut, seeds = [b"token_vault", vault_state.key().as_ref()], bump = vault_state.token_vault_bump)]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"voter_record", voter.key().as_ref()],
        bump = voter_record.bump
    )]
    pub voter_record: Account<'info, VoterRecord>,

    #[account(mut)]
    pub voter_token_account: Account<'info, TokenAccount>,

    pub voter: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SweepVault<'info> {
    #[account(
        mut,
        seeds = [b"vault_state"],
        bump = vault_state.bump,
        has_one = master @ VoteError::Unauthorized,
        close = master
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(mut, seeds = [b"token_vault", vault_state.key().as_ref()], bump = vault_state.token_vault_bump)]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub master_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub master: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// State Accounts
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct VaultState {
    pub master: Pubkey,
    pub mint: Pubkey,
    pub total_vote_amount: u64,
    pub is_refund_enabled: bool,
    pub refund_start_time: i64,
    pub refund_duration: i64,
    pub bump: u8,
    pub token_vault_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct ReferrerState {
    pub referrer: Pubkey,
    #[max_len(8)]
    pub ref_code: String,
    pub total_referred_votes: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct VoterRecord {
    pub voter: Pubkey,
    pub contribution_amount: u64,
    pub bump: u8,
}

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

#[error_code]
pub enum VoteError {
    #[msg("Unauthorized action.")]
    Unauthorized,
    #[msg("Voting is closed.")]
    VotingClosed,
    #[msg("Refunds are not currently active.")]
    RefundNotActive,
    #[msg("No contributions available to refund.")]
    NoContributionToRefund,
    #[msg("Refund window is still active.")]
    RefundWindowStillActive,
    #[msg("Invalid vote amount.")]
    InvalidAmount,
    #[msg("Referral code exceeds max length.")]
    InvalidRefCode,
}
