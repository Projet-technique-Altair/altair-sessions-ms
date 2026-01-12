
### 0. Lancer altair-infra
docker compose up postgres

### 1. Créer l'image docker
cd altair-sessions-ms
docker build -t altair-sessions-ms .

### 2. Lancer l'image
docker run --rm -it \
  --network altair-infra_default \
  -p 3003:3003 \
  --name altair-sessions-ms \
  altair-sessions-ms




