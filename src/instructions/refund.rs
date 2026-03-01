use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, ProgramResult,
};

use crate::state::Escrow;

pub fn process_refund_instruction(accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    let [maker, mint_a, escrow_account, maker_ata, escrow_ata, system_program, token_program, _associated_token_program @ ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Verify maker is signer
    if !maker.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Deserialize escrow state and verify maker
    let escrow_state = Escrow::from_account_info(escrow_account)?;
    if escrow_state.maker().as_ref() != maker.address().as_ref() {
        return Err(ProgramError::IllegalOwner);
    }
    if escrow_state.mint_a().as_ref() != mint_a.address().as_ref() {
        return Err(ProgramError::InvalidAccountData);
    }

    let bump = [data[0]];
    let seed = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(&bump),
    ];
    let seeds = Signer::from(&seed);

    // Get vault balance in scoped block to drop Ref borrow before CPI
    let vault_balance = {
        let vault_state = pinocchio_token::state::TokenAccount::from_account_view(escrow_ata)?;
        vault_state.amount()
    };

    if vault_balance > 0 {
        pinocchio_token::instructions::Transfer {
            from: escrow_ata,
            to: maker_ata,
            authority: escrow_account,
            amount: vault_balance,
        }
        .invoke_signed(&[seeds.clone()])?;
    }

    pinocchio_token::instructions::CloseAccount {
        account: escrow_ata,
        destination: escrow_account,
        authority: escrow_account,
    }
    .invoke_signed(&[seeds.clone()])?;

    // Close escrow account: move lamports to maker, then close
    let escrow_lamports = escrow_account.lamports();
    maker.set_lamports(maker.lamports() + escrow_lamports);
    escrow_account.set_lamports(0);
    escrow_account.close()?;

    Ok(())
}
