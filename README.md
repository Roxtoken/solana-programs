### Vote Competition
#Anchor Program

Simply
opens a PDA VoteVault to keep VoteTokens safe, 
accounts for ReffererVotes, 
accounts for Voter Contributions, 
Refund = True constraint { Competition Duration? }, 
Master closes competition raking back Rents and any left Tokens, 

ToDo
Vote state 
Event on vote for leaderboard indexing purposes?
Duration change:Use start time - end time for competition


#UI

Wallet connection
Gated Token Amount needed to register a refferalCode
A Marketer Modal signup Form with embedded Action URL taking inputs before generating Unique code for Marketer ActionURL+RefCode
Generator script simply uses all valid four letter names + three numbers in 000-999 Range to generate a unique Reffer Code.

#Solana Action 

Public Voter Votes by sending a token per vote with no restriction on amount and times voted
All VoteTokens is Returned to Voter after Competition
Integration

~~~

import { PublicKey, Transaction } from "@solana/web3.js";
import { Program, AnchorProvider } from "@coral-xyz/anchor";

// Derive Referrer PDA based on ref parameter
const refCode = "WORD123"; // Extracted from Action URL query string
const [referrerStatePda] = PublicKey.findProgramAddressSync(
  [Buffer.from("referrer"), Buffer.from(refCode)],
  PROGRAM_ID
);

const [vaultStatePda] = PublicKey.findProgramAddressSync(
  [Buffer.from("vault_state")],
  PROGRAM_ID
);

const [voterRecordPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("voter_record"), voterPubkey.toBuffer()],
  PROGRAM_ID
);

// Build instruction from Action Endpoint
const ix = await program.methods
  .vote(voteAmountBN)
  .accounts({
    vaultState: vaultStatePda,
    tokenVault: tokenVaultPda,
    referrerState: referrerStatePda,
    voterRecord: voterRecordPda,
    voterTokenAccount: voterSPLAccount,
    voter: voterPubkey,
    tokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  })
  .instruction();

~~~
