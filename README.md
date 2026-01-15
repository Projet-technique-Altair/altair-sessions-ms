
# Altair Sessions Microservice

## Sessions

### 0. Start infrastructure (database required)

```bash
docker compose up postgres
```

---

### 1. Build the Docker image (ONLY if code changed)

Use this step if:

* you modified the sessions code
* you modified the Dockerfile
* first run on a new machine

```bash
cd altair-sessions-ms
docker build -t altair-sessions-ms .
```

---

### 2. Run the service

```bash
docker run --rm -it \
  --network altair-infra_default \
  -p 3003:3003 \
  --name altair-sessions-ms \
  altair-sessions-ms
```

---

### Notes

* The service is meant to be destroyed when the terminal closes : rebuild necessary
* This service is accessed ONLY by the API Gateway
* The frontend must never call this service directly
