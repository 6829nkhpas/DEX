import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { StoreProvider } from "./state/StoreProvider";
import { WalletProvider } from "./wallet/WalletProvider";

import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
        <WalletProvider>
            <StoreProvider>
                <App />
            </StoreProvider>
        </WalletProvider>
    </React.StrictMode>
);
