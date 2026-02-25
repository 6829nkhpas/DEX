import React, { createContext, useContext, useEffect, useState, useMemo } from "react";
import { DexStateStore } from "./store";
import { DexWebSocketClient } from "../ws/ws-client";
import { StoreState } from "./types";

interface DexContextValue {
    store: DexStateStore;
    client: DexWebSocketClient;
    state: StoreState;
    connectionStatus: "disconnected" | "connecting" | "connected" | "error";
}

const DexContext = createContext<DexContextValue | null>(null);

export function StoreProvider({ children }: { children: React.ReactNode }) {
    // Initialize singletons once
    const { store, client } = useMemo(() => {
        const store = new DexStateStore();

        // Use a mock token for development
        const client = new DexWebSocketClient({
            url: "ws://localhost:8080/v1/ws",
            getToken: async () => process.env.VITE_WS_TOKEN || "dev-token-123",
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

    // Reactive state
    const [state, setState] = useState<StoreState>(store.getState());
    const [connectionStatus, setConnectionStatus] = useState<DexContextValue["connectionStatus"]>("disconnected");

    // Establish connection and listen to state changes
    useEffect(() => {
        // Subscribe to store updates
        const unsubscribe = store.onStateChange((newState) => {
            setState({ ...newState });
        });

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
            unsubscribe();
            client.disconnect();
        };
    }, [client, store]);

    const value: DexContextValue = {
        store,
        client,
        state,
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
