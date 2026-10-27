import type { NextConfig } from "next";

// Caddy serves out/ and proxies /api and /uploads to the Rust API.
const config: NextConfig = {
  output: "export",
  trailingSlash: true,
};

export default config;
