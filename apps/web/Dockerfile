FROM node:20-bookworm
WORKDIR /app
COPY apps/web/package.json package.json
RUN npm install
COPY apps/web .
EXPOSE 5173
CMD ["npm", "run", "dev"]

