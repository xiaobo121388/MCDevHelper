import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { DialogApp } from "./DialogApp";
import { dialogRequest } from "./windows";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {dialogRequest ? <DialogApp request={dialogRequest} /> : <App />}
  </StrictMode>,
);
