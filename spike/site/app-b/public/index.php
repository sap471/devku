<?php
$uri = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH);
header('Content-Type: text/plain');
echo "site=app-b\n";
echo "php=" . PHP_VERSION . "\n";
echo "uri=" . $uri . "\n";
echo "ext_mbstring=" . (extension_loaded('mbstring') ? 'yes' : 'no') . "\n";
