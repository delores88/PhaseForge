/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "export",
  reactStrictMode: true,
  devIndicators: false,
  poweredByHeader: false,
  // Release evidence binds shipped chunks to their emitted source-map content.
  productionBrowserSourceMaps: true,
  trailingSlash: true,
  images: {
    unoptimized: true,
  },
};

module.exports = nextConfig;
