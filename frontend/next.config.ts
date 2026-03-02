import type { NextConfig } from "next";

const nextConfig: NextConfig =
  process.env.NODE_ENV === "development"
    ? {
        // Dev mode: proxy /api and /ws to the Rust backend
        images: { unoptimized: true },
        async rewrites() {
          return [
            {
              source: "/api/:path*",
              destination: "http://localhost:3001/api/:path*",
            },
            {
              source: "/ws/:path*",
              destination: "http://localhost:3001/ws/:path*",
            },
          ];
        },
      }
    : {
        // Production: static export served by the Rust backend
        output: "export",
        distDir: "out",
        trailingSlash: true,
        images: { unoptimized: true },
      };

export default nextConfig;
