import pytest

class EscrowContract:
    """
    Simulated Escrow Contract validating close and withdraw mechanisms to prevent permanent locking.
    """
    def __init__(self, owner: str, bounty_amount: float):
        self.owner = owner
        self.bounty_amount = bounty_amount
        self.is_closed = False
        self.funds_locked = True

    def close_and_withdraw(self, requester: str) -> float:
        if self.is_closed:
            raise RuntimeError("Escrow contract is already closed.")
        if requester != self.owner:
            raise PermissionError("Only the escrow owner can close and withdraw funds.")
        
        self.is_closed = True
        self.funds_locked = False
        withdrawn = self.bounty_amount
        self.bounty_amount = 0.0
        return withdrawn

def test_escrow_close_and_withdraw_success():
    """Verify escrow owner can successfully close and withdraw funds without permanent lock."""
    escrow = EscrowContract(owner="MochiGem", bounty_amount=100.0)
    assert escrow.funds_locked is True
    
    amount_withdrawn = escrow.close_and_withdraw(requester="MochiGem")
    assert amount_withdrawn == 100.0
    assert escrow.is_closed is True
    assert escrow.funds_locked is False
    assert escrow.bounty_amount == 0.0

def test_escrow_close_unauthorized_rejection():
    """Verify unauthorized requesters cannot close escrow."""
    escrow = EscrowContract(owner="MochiGem", bounty_amount=100.0)
    
    with pytest.raises(PermissionError, match="Only the escrow owner"):
        escrow.close_and_withdraw(requester="unauthorized_actor")
    
    assert escrow.funds_locked is True
    assert escrow.is_closed is False
