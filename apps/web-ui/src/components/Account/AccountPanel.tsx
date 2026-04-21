// ---------------------------------------------------------------------------
// AccountPanel — live balance display with deposit / withdraw stubs
// ---------------------------------------------------------------------------
// Phase 15: redesigned with glass-panel, loading skeleton, consistent table,
//           improved auth status indicator, and better disabled states.
// ---------------------------------------------------------------------------

import React, { useState } from "react";
import { useDexStore, useAppSelector } from "../../state/StoreProvider";
import { useWallet } from "../../wallet/WalletProvider";
import { useAuth } from "../../auth/AuthProvider";
import { DepositModal } from "./DepositModal";
import { WithdrawModal } from "./WithdrawModal";
import { StatusIndicator } from "../ui/StatusIndicator";
import { LoadingSkeleton } from "../ui/LoadingSkeleton";
import { EmptyState } from "../ui/EmptyState";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AccountPanel: React.FC = () => {
  
  const { address, accountId } = useWallet();
  const { authStatus } = useAuth();
  const isAuthenticated = authStatus === "authenticated";
  const [showDeposit, setShowDeposit] = useState(false);
  const [showWithdraw, setShowWithdraw] = useState(false);

  const account = useAppSelector(state => state.account);

  if (!address) {
    return (
      <div className="glass-panel rounded-2xl p-5 border-t border-indigo-500/20" style={{ gridArea: "account" }}>
        <EmptyState icon="wallet" message="Connect a wallet to view balances" />
      </div>
    );
  }

  const balances = account?.balances ?? {};
  const assets = Object.keys(balances).sort();

  return (
    <div className="glass-panel rounded-2xl p-5 border-t border-indigo-500/20 flex flex-col gap-3" style={{ gridArea: "account" }}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <span className="panel-header">Account</span>
        <span
          className="text-[10px] text-slate-500 font-mono truncate max-w-[140px]"
          title={accountId ?? ""}
        >
          {accountId ? `${accountId.slice(0, 8)}…` : "—"}
        </span>
      </div>

      {/* Auth status */}
      <div className="flex items-center gap-1.5">
        <StatusIndicator
          status={isAuthenticated ? "connected" : "warning"}
          label={isAuthenticated ? "Authenticated" : "Sign in to trade"}
          size="sm"
        />
      </div>

      {/* Balance table */}
      {!account ? (
        <LoadingSkeleton variant="card" />
      ) : assets.length === 0 ? (
        <EmptyState message="No balances yet" icon="empty" />
      ) : (
        <div className="overflow-hidden rounded-lg border border-indigo-500/10 bg-slate-900/40">
          <table className="data-table">
            <thead>
              <tr>
                <th className="text-left">Asset</th>
                <th className="text-right">Balance</th>
              </tr>
            </thead>
            <tbody>
              {assets.map((asset) => (
                <tr key={asset}>
                  <td className="font-mono text-white font-semibold text-xs">{asset}</td>
                  <td className="text-right font-mono text-slate-300 tabular-nums text-xs">
                    {balances[asset]}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Actions */}
      <div className="flex gap-2 mt-1">
        <button
          onClick={() => setShowDeposit(true)}
          className="btn-action btn-action-buy flex-1 py-2 text-xs"
        >
          Deposit
        </button>
        <button
          onClick={() => isAuthenticated && setShowWithdraw(true)}
          disabled={!isAuthenticated}
          title={!isAuthenticated ? "Sign in to withdraw" : undefined}
          className="btn-action btn-action-danger flex-1 py-2 text-xs"
        >
          Withdraw
        </button>
      </div>

      {/* Modals */}
      {showDeposit && (
        <DepositModal onClose={() => setShowDeposit(false)} />
      )}
      {showWithdraw && (
        <WithdrawModal onClose={() => setShowWithdraw(false)} />
      )}
    </div>
  );
};
