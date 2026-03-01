// ---------------------------------------------------------------------------
// DepositModal — stub deposit flow (no contract integration yet)
// ---------------------------------------------------------------------------

import React, { useState, useCallback } from "react";

export interface DepositModalProps {
  onClose: () => void;
}

export const DepositModal: React.FC<DepositModalProps> = ({ onClose }) => {
  const [asset, setAsset] = useState("USDT");
  const [amount, setAmount] = useState("");
  const [submitted, setSubmitted] = useState(false);

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (!amount || parseFloat(amount) <= 0) return;

      // Stub: log to console and show success
      console.log(`[DepositModal] Deposit ${amount} ${asset}`);
      setSubmitted(true);

      // Auto-close after short delay
      setTimeout(onClose, 1500);
    },
    [asset, amount, onClose],
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="bg-gray-900 border border-gray-700 rounded-lg p-6 w-80 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-sm font-bold text-white mb-4">Deposit</h3>

        {submitted ? (
          <div className="text-green-400 text-sm text-center py-4">
            ✓ Deposit of {amount} {asset} submitted (stub)
          </div>
        ) : (
          <form className="flex flex-col gap-3" onSubmit={handleSubmit}>
            <label className="text-xs text-gray-400">
              Asset
              <select
                value={asset}
                onChange={(e) => setAsset(e.target.value)}
                className="mt-1 block w-full bg-gray-800 border border-gray-700 text-white text-sm rounded p-2"
              >
                <option value="USDT">USDT</option>
                <option value="BTC">BTC</option>
                <option value="ETH">ETH</option>
                <option value="SOL">SOL</option>
              </select>
            </label>

            <label className="text-xs text-gray-400">
              Amount
              <input
                type="text"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                placeholder="0.00"
                className="mt-1 block w-full bg-gray-800 border border-gray-700 text-white text-sm rounded p-2"
              />
            </label>

            <div className="flex gap-2 mt-2">
              <button
                type="submit"
                className="flex-1 py-2 text-sm font-medium bg-green-700 hover:bg-green-600 text-white rounded transition-colors"
              >
                Deposit
              </button>
              <button
                type="button"
                onClick={onClose}
                className="flex-1 py-2 text-sm font-medium bg-gray-700 hover:bg-gray-600 text-white rounded transition-colors"
              >
                Cancel
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
};
