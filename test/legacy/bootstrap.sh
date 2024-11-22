#!/usr/bin/env bash

declare -A KEYS
KEYS['summit']=5zIaXc6Cn9qEAk2rNcyu-KDVtdFGcJtx9p2gZdDaxhU
KEYS['vessel']=uxV-S0soSdALp8G-IVbjJMmATzTCfA-8abkNbJ7PKt8
KEYS['avalanche']=ZZFERyM1phMGH1PQ0AeoQ0KNE7-cS-O7oEYtPVqr36M

declare -A PORTS
PORTS['summit']=5000
PORTS['vessel']=5001
PORTS['avalanche']=5002

USER=test
PASS=test1234

echo "Starting bootstrap"

sleep 2

cookies=$(mktemp)

echo "Creating summit account"
curl "http://127.0.0.1:${PORTS['summit']}/setup" \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-raw "instanceURI=http%3A%2F%2Fsummit%3A${PORTS['summit']}&description=&username=$USER&emailAddress=admin%40admin.com&password=$PASS&confirmPassword=$PASS"

echo "Logging into summit"
curl "http://127.0.0.1:${PORTS['summit']}/accounts/login" \
  -b $cookies \
  -c $cookies \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-raw "username=$USER&password=$PASS"

echo "Adding avalanche"
curl "http://127.0.0.1:${PORTS['summit']}/api/v1/builders/create" \
  -b $cookies \
  -H 'Content-Type: application/json' \
  --data-raw '{"request":{"id":"avalanche","summary":"Builds stuff","instanceURI":"http://avalanche:'"${PORTS['avalanche']}"'","pubkey":"'"${KEYS['avalanche']}"'","adminName":"admin","adminEmail":"admin@admin.com"}}'

echo "Adding vessel"
curl "http://127.0.0.1:${PORTS['summit']}/api/v1/endpoints/create" \
  -b $cookies \
  -H 'Content-Type: application/json' \
  --data-raw '{"request":{"id":"official","summary":"Indexes stuff","instanceURI":"http://vessel:'"${PORTS['vessel']}"'","pubkey":"'"${KEYS['vessel']}"'","adminName":"admin","adminEmail":"admin@admin.com"}}'

sleep 1

echo "Restarting summit"
docker compose restart summit
