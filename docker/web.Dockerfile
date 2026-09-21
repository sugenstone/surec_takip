FROM node:24-bookworm-slim AS development
WORKDIR /workspace
COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/package.json
COPY packages/ui/package.json packages/ui/package.json
COPY packages/contracts/package.json packages/contracts/package.json
RUN npm ci --no-audit --no-fund
COPY apps/web apps/web
COPY packages/ui packages/ui
COPY packages/contracts packages/contracts
EXPOSE 5173
CMD ["npm", "run", "dev", "--workspace", "@platform/web", "--", "--host", "0.0.0.0", "--port", "5173", "--strictPort"]

FROM development AS build
RUN npm run build

FROM node:24-bookworm-slim AS runtime
WORKDIR /app
COPY --from=build --chown=node:node /workspace/apps/web/build ./build
COPY --from=build --chown=node:node /workspace/node_modules ./node_modules
USER node
ENV HOST=0.0.0.0 PORT=3000
EXPOSE 3000
CMD ["node", "build/index.js"]
