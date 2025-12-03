#!/bin/bash

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${YELLOW}=== Push Subscriptions E2E Test ===${NC}\n"

# Cleanup
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    [ ! -z "$LCLQ_PID" ] && kill $LCLQ_PID 2>/dev/null
    [ ! -z "$WEBHOOK_PID" ] && kill $WEBHOOK_PID 2>/dev/null
    rm -f /tmp/webhook_messages.json /tmp/webhook_server.py
    echo -e "${GREEN}Done${NC}"
}
trap cleanup EXIT

# Start webhook server
echo -e "${YELLOW}1. Starting webhook server...${NC}"
cat > /tmp/webhook_server.py <<'EOF'
#!/usr/bin/env python3
import json, base64
from http.server import HTTPServer, BaseHTTPRequestHandler

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers['Content-Length']))
        payload = json.loads(body)

        print("\n" + "="*60)
        print("📨 RECEIVED PUSH NOTIFICATION")
        print("="*60)
        print(f"Subscription: {payload['subscription']}")
        msg = payload['message']
        print(f"Message ID: {msg['messageId']}")
        print(f"Data: {base64.b64decode(msg['data']).decode()}")
        print(f"Attributes: {json.dumps(msg.get('attributes', {}))}")
        print("="*60)

        with open('/tmp/webhook_messages.json', 'w') as f:
            json.dump(payload, f)

        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'OK')

    def log_message(self, *args): pass

HTTPServer(('127.0.0.1', 8888), Handler).serve_forever()
EOF

python3 /tmp/webhook_server.py &
WEBHOOK_PID=$!
sleep 1
echo -e "${GREEN}✓ Webhook running on http://127.0.0.1:8888 (PID: $WEBHOOK_PID)${NC}\n"

# Start lclq
echo -e "${YELLOW}2. Starting lclq server...${NC}"
LCLQ_RECEIPT_SECRET="test-secret" LCLQ_PUSH_WORKERS=2 ./target/release/lclq start &
LCLQ_PID=$!
sleep 3
echo -e "${GREEN}✓ lclq started (PID: $LCLQ_PID)${NC}\n"

# Create topic
echo -e "${YELLOW}3. Creating topic...${NC}"
curl -s -X PUT http://localhost:8086/v1/projects/test/topics/e2e-test \
  -H "Content-Type: application/json" -d '{}' > /dev/null
echo -e "${GREEN}✓ Topic created${NC}\n"

# Create push subscription
echo -e "${YELLOW}4. Creating push subscription...${NC}"
curl -s -X PUT http://localhost:8086/v1/projects/test/subscriptions/e2e-sub \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "projects/test/topics/e2e-test",
    "pushConfig": {"pushEndpoint": "http://127.0.0.1:8888/webhook"},
    "retryPolicy": {"minimumBackoff": "1s", "maximumBackoff": "5s"}
  }' > /dev/null
echo -e "${GREEN}✓ Push subscription created${NC}\n"

# Publish message
echo -e "${YELLOW}5. Publishing message...${NC}"
DATA=$(echo -n "Hello from lclq push!" | base64)
curl -s -X POST http://localhost:8086/v1/projects/test/topics/e2e-test:publish \
  -H "Content-Type: application/json" \
  -d "{\"messages\":[{\"data\":\"$DATA\",\"attributes\":{\"source\":\"test\"}}]}" > /dev/null
echo -e "${GREEN}✓ Message published${NC}\n"

# Wait for delivery
echo -e "${YELLOW}6. Waiting for push delivery...${NC}"
for i in {1..30}; do
    if [ -f /tmp/webhook_messages.json ]; then
        echo -e "${GREEN}✓ Delivered!${NC}\n"
        break
    fi
    sleep 0.5
    echo -n "."
done
echo ""

# Verify
if [ -f /tmp/webhook_messages.json ]; then
    RECEIVED=$(cat /tmp/webhook_messages.json | jq -r '.message.data' | base64 -d)
    if [ "$RECEIVED" = "Hello from lclq push!" ]; then
        echo -e "${GREEN}╔═══════════════════════════════╗${NC}"
        echo -e "${GREEN}║  🎉 TEST PASSED! 🎉           ║${NC}"
        echo -e "${GREEN}╚═══════════════════════════════╝${NC}\n"
        echo "✓ Server started with 2 workers"
        echo "✓ Topic created"
        echo "✓ Push subscription registered"
        echo "✓ Message published"
        echo "✓ Webhook received push notification"
        echo "✓ Message content verified"
        exit 0
    else
        echo -e "${RED}✗ Message content mismatch${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ Message not delivered${NC}"
    exit 1
fi
