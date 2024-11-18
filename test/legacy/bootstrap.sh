#!/usr/bin/env bash

SERVICES=(summit avalanche)

declare -A KEYS
KEYS['summit']=5zIaXc6Cn9qEAk2rNcyu-KDVtdFGcJtx9p2gZdDaxhU
KEYS['vessel']=uxV-S0soSdALp8G-IVbjJMmATzTCfA-8abkNbJ7PKt8
KEYS['avalanche']=C64M-DMlib7vl_DAFRPAkKzok6cJ2el1fMxd-LdlGZ0

declare -A PORTS
PORTS['summit']=5000
PORTS['vessel']=5001
PORTS['avalanche']=5002

USER=test
PASS=test1234

echo "Starting bootstrap"

sleep 2

cookies=$(mktemp)

for service in ${SERVICES[*]}
do
  port=${PORTS[$service]}

  case $service in
    summit)
      path="setup"
      ;;
    *)
      path="setup/apply"
      ;;
  esac

  echo "Creating $service account"
  curl "http://127.0.0.1:$port/$path" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-raw "instanceURI=http%3A%2F%2F$service%3A$port&description=&username=$USER&emailAddress=admin%40admin.com&password=$PASS&confirmPassword=$PASS"
  
  echo "Logging into $service"
  curl "http://127.0.0.1:$port/accounts/login" \
    -b $cookies \
    -c $cookies \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-raw "username=$USER&password=$PASS"
done

echo "Adding avalanche"
curl "http://127.0.0.1:${PORTS['summit']}/api/v1/builders/create" \
  -b $cookies \
  -H 'Content-Type: application/json' \
  --data-raw '{"request":{"id":"avalanche","summary":"Builds stuff","instanceURI":"http://avalanche:'"${PORTS['avalanche']}"'","pubkey":"'"${KEYS['avalanche']}"'","adminName":"admin","adminEmail":"admin@admin.com"}}'
curl -b $cookies "http://127.0.0.1:${PORTS['avalanche']}/avl/accept/${KEYS['summit']}" 

echo "Adding vessel"
curl "http://127.0.0.1:${PORTS['summit']}/api/v1/endpoints/create" \
  -b $cookies \
  -H 'Content-Type: application/json' \
  --data-raw '{"request":{"id":"official","summary":"Indexes stuff","instanceURI":"http://vessel:'"${PORTS['vessel']}"'","pubkey":"'"${KEYS['vessel']}"'","adminName":"admin","adminEmail":"admin@admin.com"}}'
# vessel-rs auto-accepts
# curl -b $cookies "http://127.0.0.1:${PORTS['vessel']}/vsl/accept/${KEYS['summit']}" 

# echo "Importing stones"
# curl -b $cookies "http://127.0.0.1:${PORTS['vessel']}/vsl/import?importPath=%2Fimport"

echo "Restarting summit"
docker compose restart summit
