import os
import glob

def refactor(file):
    with open(file, 'r') as f:
        content = f.read()

    # Replaces imports
    if 'useDexStore' in content and 'useAppSelector' not in content:
        content = content.replace('import { useDexStore }', 'import { useDexStore, useAppSelector }')
        content = content.replace('import {useDexStore}', 'import { useDexStore, useAppSelector }')

    # Replace usages
    # 1. DebugPanel
    if "DebugPanel" in file:
        content = content.replace('const { state, connectionStatus } = useDexStore();', 'const { connectionStatus } = useDexStore();\n    const state = useAppSelector(state => state);')
    # 2. AccountPanel
    elif "AccountPanel" in file:
        content = content.replace('const { state } = useDexStore();', 'const account = useAppSelector(state => state.account);')
        # Replace state.account to account
        content = content.replace('state.account', 'account')
    # 3. RiskPage
    elif "RiskPage" in file:
        content = content.replace('const { state } = useDexStore();', 'const account = useAppSelector(state => state.account);')
        content = content.replace('state.account', 'account')
    # 4. Orderbook
    elif "Orderbook/Orderbook.tsx" in file:
        content = content.replace('const { state } = useDexStore();\n    const orderbook = state.orderbooks.get(symbol);', 'const orderbook = useAppSelector(state => state.orderbooks.get(symbol));')
        # Also clean up empty space if there was any left over
    # 5. TickerPanel
    elif "TickerPanel" in file:
        content = content.replace('const { state } = useDexStore();\n    const ticker = state.tickers.get(symbol);', 'const ticker = useAppSelector(state => state.tickers.get(symbol));')
    # 6. TradeTape
    elif "TradeTape" in file:
        content = content.replace('const { store, state } = useDexStore();\n    const trades = state.trades.get(symbol) || [];', 'const trades = useAppSelector(state => state.trades.get(symbol)) || [];')
        # Store might still be needed? TradeTape doesn't use store actually, except maybe we just removed state from it.
        # Wait, if trade tape doesn't use store
        content = content.replace('const { store, state } = useDexStore();', 'const { store } = useDexStore();')
    # 7. OpenOrders
    elif "OpenOrders" in file:
        content = content.replace('const { state } = useDexStore();', 'const account = useAppSelector(state => state.account);')
        content = content.replace('state.account', 'account')
    # 8. Positions
    elif "Positions" in file:
        content = content.replace('const { state } = useDexStore();', '''const { store } = useDexStore();
    // Re-render when mark prices for our positions change
    useAppSelector(state => 
        positions.map(p => state.tickers.get(p.symbol)?.mark_price).join(',')
    );

    const state = store.getState();''')

    # MarketPage
    elif "MarketPage" in file:
        content = content.replace('state.metrics', 'useAppSelector(s => s.metrics)')

    # OrderEntry - no direct state usage from context, it has const { store } = useDexStore() which is fine.

    with open(file, 'w') as f:
        f.write(content)

for file in glob.glob('/home/nkh/finalyearproject/apps/web-ui/src/components/**/*.tsx', recursive=True):
    refactor(file)

for file in glob.glob('/home/nkh/finalyearproject/apps/web-ui/src/pages/*.tsx', recursive=True):
    refactor(file)

