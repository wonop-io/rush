#!/bin/bash

echo "Testing that Stripe starts only once"
echo "===================================="
echo ""

# Run dev command for 15 seconds and capture output
echo "Starting dev environment for 15 seconds..."
./rush/target/release/rush io.wonop.helloworld dev --output-format split 2>&1 | head -200 > /tmp/rush_output.txt &
PID=$!

sleep 15
kill $PID 2>/dev/null
wait $PID 2>/dev/null

echo ""
echo "Analyzing output for Stripe startup messages..."
echo ""

# Count how many times Stripe is registered
REGISTER_COUNT=$(grep -c "Registering Stripe CLI service" /tmp/rush_output.txt)
echo "Stripe registration count: $REGISTER_COUNT"

# Count how many times Stripe starts  
START_COUNT=$(grep -c "Starting Stripe CLI" /tmp/rush_output.txt)
echo "Stripe start count: $START_COUNT"

# Look for duplicate starts
echo ""
echo "All Stripe-related log lines:"
echo "------------------------------"
grep -i stripe /tmp/rush_output.txt | grep -v "STRIPE_" | head -20

echo ""
if [ "$REGISTER_COUNT" -eq 1 ] && [ "$START_COUNT" -le 1 ]; then
    echo "✅ SUCCESS: Stripe is only started once!"
else
    echo "❌ FAILURE: Stripe is started multiple times!"
    echo "   Expected: 1 registration, 0-1 starts"
    echo "   Got: $REGISTER_COUNT registrations, $START_COUNT starts"
fi

echo ""
echo "Cleaning up..."
rm -f /tmp/rush_output.txt