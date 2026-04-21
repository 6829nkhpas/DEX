import React, { createContext, useContext, useEffect, useState, useMemo, useSyncExternalStore } from "react";
import { DexStateStore } from "./store";
import { DexWebSocketClient } from "../ws/ws-client";
import { StoreState } from "./types";
import { getConfig } from "../infra/config";

interface DexContextValue {
    store: DexStateStore;
    client: DexWebSocketClient;
    connectionStatus: "disconnected" | "connecting" | "connected" | "error";
}

const DexContext = createContext<DexContextValue | null>(null);

export function StoreProvider({ children }: { children: React.ReactNode }) {
    // Initialize singletons once
    const { store, client } = useMemo(() => {
        const config = getConfig();
        const store = new DexStateStore();

        // Token from environment config — no hardcoded secrets
        const client = new DexWebSocketClient({
            url: config.wsUrl,
            getToken: async () => config.wsToken,
        });

        // Wire WS events -> Store dispatch
        client.onEvent("market_data", (event) => store.dispatch(event));
        client.onEvent("trades", (event) => store.dispatch(event));
        client.onEvent("account", (event) => store.dispatch(event));

        // Wire Store missing snapshot -> WS snapshot request
        store.onRequestSnapshot((channel, params, sinceSeq) => {
            // Re-subscribe or send explict request (using the robust WS client)
            // The WS client natively handles gaps when seeing higher seqs, 
            // but if store cap exceeded we can force fetch:
            client.subscribe(channel as any, params);
        });

        return { store, client };
    }, []);

    // Reactive Status (State is handled by selectors now)
    const [connectionStatus, setConnectionStatus] = useState<DexContextValue["connectionStatus"]>("disconnected");

    // Establish connection
    useEffect(() => {

        // Manage WS connection internally (simulated connection logic for now)
        let mounted = true;

        const connect = async () => {
            try {
                setConnectionStatus("connecting");
                await client.connect();
                if (mounted) setConnectionStatus("connected");
            } catch (e) {
                if (mounted) setConnectionStatus("error");
            }
        };

        client.onError((code, msg) => {
            console.error(`WS Error [${code}]: ${msg}`);
            setConnectionStatus("error");
        });

        connect();

        return () => {
            mounted = false;
            client.disconnect();
        };
    }, [client, store]);

    const value: DexContextValue = {
        store,
        client,
        connectionStatus
    };

    return <DexContext.Provider value={value}>{children}</DexContext.Provider>;
}

export function useDexStore(): DexContextValue {
    const context = useContext(DexContext);
    if (!context) {
        throw new Error("useDexStore must be used within a StoreProvider");
    }
    return context;
}

/**
 * Precision selector hook using useSyncExternalStore.
 * Prevents cascading React re-renders by only re-rendering when the returned slice changes.
 */
export function useAppSelector<T>(selector: (state: StoreState) => T): T {
    const { store } = useDexStore();
    return useSyncExternalStore(
        (listener: () => void) => store.onStateChange(listener),
        () => selector(store.getState())
    );
}
