import { contextBridge } from "electron";
import type { BananaApi } from "../shared/api";

const api: BananaApi = Object.freeze({ platform: process.platform });

contextBridge.exposeInMainWorld("banana", api);
