# solana-programs
Here lies my Solana-Program-Examples

Architecture Overview
This Solana dApp architecture relies on a Primary Treasury Account that acts as an un-ruggable custody vault. It funnels funds exclusively into ephemeral, program-derived NameEvent Accounts. These secondary accounts handle localized reward distribution and public interactions (via Blinks and web actions), then safely sweep remaining funds back to a Primary Reward Account upon completion.
Summary
Treat program like a factory, not a storage room. The factory code knows how to build a NameEvent, Shop or Campaign ect, but the actual items are built on-demand, used, and closed out dynamically.

Core Account Models (Anchor Framework)
1. Primary Treasury (Vault State)
A global config account initialized once by the protocol admin. It accepts deposits from anyone via a standard token transfer, but its authority cannot withdraw funds to an external wallet—it can only initialize and fund approved NameEvent accounts.
'''
Rust

use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[account]
pub struct PrimaryTreasury {
    pub admin: Pubkey,
    pub primary_reward_account: Pubkey, // Destination for swept leftover funds
    pub vault_token_account: Pubkey,    // Associated Token Account holding funds
    pub bump: u8,
}
'''
2. NameEvent Account (Ephemeral State)
Created dynamically for each new event. It tracks active participants (Earners) and holds a localized balance allocated from the Primary Treasury. This account is fully closable.

Rust
#[account]
pub struct NameEvent {
    pub treasury: Pubkey,
    pub event_id: u64,
    pub total_allocated: u64,
    pub bump: u8,
    pub is_active: bool,
}
Program Instructions & Workflow
Step 1: Fund the Primary Treasury
Instruction: deposit_to_treasury(amount: u64)

Behavior: Any contributor calls this instruction to transfer SPL tokens/USDC into the PrimaryTreasury vault token account. No withdrawal counterpart exists in the program logic.

Step 2: Initialize a NameEvent
Instruction: initialize_name_event(event_id: u64, allocation_amount: u64)

Behavior:

Derives a PDA for the NameEvent.

Transfers allocation_amount from the PrimaryTreasury vault directly into the event's designated escrow account.

Sets is_active = true.

Step 3: Public Interactions & Earners Registration (Blinks & Web)
Earners Registration: Web-based interface allows community wallet accounts to register as reward earners for the specific event_id.

Public Payers/Spenders (Blinks): Actions map to an instruction like process_event_interaction, where users interact via Solana Actions (Blinks) to trigger reward payouts or spend flows from the active NameEvent escrow.

Step 4: Close Event & Sweep Remaining Funds
Instruction: close_name_event()

Behavior:

Can be called when an event finishes or fails/cancels.

Calculates remaining rent lamports and token balances in the NameEvent escrow.

Sweeps all remaining tokens directly back to the Primary Reward Account.

Closes the NameEvent account data structure and returns its rent lamports to the admin or treasury.

Sets is_active = false, leaving the system ready to spin up a new event_id.

Solana Actions & Blinks Integration
To expose public interactions and earner registrations via Blinks (dialects of Solana Actions), your API server needs to serve endpoints conforming to the Solana Actions spec (GET for metadata, POST for transaction generation).

Earner Registration Blink: Exposes a URL that prompts a user wallet to sign a registration instruction (register_earner) appending their community wallet account to the active NameEvent.

Payer/Spender Blink: Exposes a UI card for public contributors to interact with the active event (e.g., funding a specific milestone or triggering a spend action), routing funds through the active NameEvent logic.

1. Generalizing "Events" into "Modules" or "Activity Accounts"Instead of hardcoding the program to only think about a single type of "NameEvent," you can design your secondary accounts to be more generic.A NameEvent, a Shop, and a Trade Area can all share the exact same underlying lifecycle pattern: Initialize with funds from the treasury $\rightarrow$ Run interactive logic (Blinks/Web) $\rightarrow$ Close and sweep remaining tokens back to the Primary Reward Account.In your code, you can use an enum to define the type of secondary account being spun up:
2.Rust#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum ActivityType {
    NameEvent,
    Shop,
    TradeArea,
}
4. Making the Primary Treasury "Updateable"Your Primary Treasury account holds global configurations (like who the admin is, where leftover rewards go, or what features are enabled). To make it updateable, you want an instruction like update_treasury_config:The Admin Control: Only the designated admin key can sign and update parameters.Adding New Whitelisted Program Routines: If you roll out a new feature (like a "Shop" module), your primary treasury can store flags or configuration data pointing to what types of modules are currently allowed to request funds.Account Resizing (realloc): If your Primary Treasury account needs to store a growing list of metadata, configuration toggles, or active module trackers, Anchor makes it easy to dynamically resize the account on-chain using the realloc constraint so you never run out of space.How it flows together:Primary Treasury acts as the central bank and configuration hub (updateable by admin).When you want to launch Event #2 or open a Shop, your frontend calls initialize_activity (passing whether it's an event or a shop).The program mints a secondary PDA configured for that specific use case, funding it safely from the Treasury.When that shop or event closes down, everything settles back to the Primary Reward Account, keeping your global treasury clean and ready for the next iteration.
5. 
realloc belongs on your Primary Treasury / Config account, not the secondary NameEvent accounts.

Here is why:

Secondary NameEvent accounts are ephemeral. They are spun up, used, and then completely closed (close = ...) to reclaim rent when an event finishes. Because they have a short, fixed lifecycle, you just allocate the correct fixed size when initializing them.

The Primary Treasury / Config account is permanent. If your platform grows to add new shops, trade areas, or configuration toggles, its data structure might need to expand. realloc lets you dynamically grow its byte size on-chain without losing its address or history.

Anchor realloc Code Example
To resize an account in Anchor, use the realloc, realloc::payer, and realloc::zero constraints inside your #[derive(Accounts)] struct.

use anchor_lang::prelude::*;

#[derive(Accounts)]
// Pass the new target size as an instruction argument so the macro can read it
#[instruction(new_space: u32)] 
pub struct UpdateTreasuryConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"primary_treasury"],
        bump = treasury.bump,
        // Dynamically change the size of the account
        realloc = new_space as usize,
        realloc::payer = admin,
        realloc::zero = false, // Set to true if you want newly added bytes zeroed out
    )]
    pub treasury: Account<'info, PrimaryTreasury>,
    
    pub system_program: Program<'info, System>,
}

#[account]
pub struct PrimaryTreasury {
    pub admin: Pubkey,
    pub primary_reward_account: Pubkey,
    pub bump: u8,
    // Imagine we are adding a dynamic vector of active modules or extra config strings here later
    pub active_modules: Vec<Pubkey>, 
}

How it works:
#[instruction(new_space: u32)]: This tells Anchor to look at the instruction parameters so it can calculate the dynamic space value passed from your client.

realloc = new_space as usize: Specifies the exact new byte size the account should become. (Remember to calculate base discriminator 8 + fields, or pass a calculated size from your TypeScript/Rust client).

realloc::payer = admin: Because expanding an account requires more rent-exempt lamports, this tells Anchor who pays for the extra bytes.

Events should never be hardcoded as static entries like event1, event2, etc. Instead, they should be added dynamically as individual accounts whenever you think of them.

Hardcoding limits you because you would have to redeploy your smart contract every time you want to host a new event or open a new shop.

How Dynamic Creation Works in Practice:
Instead of fixed names, you use a unique identifier or counter stored in your Primary Treasury, combined with a Program Derived Address (PDA) seed.

Treasury Keeps a Counter: Your PrimaryTreasury account tracks an incrementing number, like next_event_id: u64 (starting at 1).

Spawning on the Fly: When you want to launch a new NameEvent, Shop, or Trade Area right now, your frontend or admin script calls an instruction like initialize_activity.

The Seed Structure: The program generates the secondary account's address using seeds like:

Rust
"seeds = [b"activity", treasury.key().as_ref(), &event_id.to_le_bytes()]"

Infinite Scaling: Whether you spin up Event #1 today, a Shop next Tuesday, and Event #2 next month, the program handles them all through the exact same logic. Each one gets its own unique, isolated account on-chain.




