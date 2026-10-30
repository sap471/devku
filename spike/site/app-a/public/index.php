<?php
$uri = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH);
if ($uri === '/slow') { usleep(2000000); }
header('Content-Type: text/plain');
echo "site=app-a php=" . PHP_VERSION . " uri=" . $uri . "\n";
echo "pid=" . getmypid() . "\n";
