#!/bin/sh

tsc --outDir build/
mkdir -p dist/assets
cp -R assets dist/
node-sass src/ -o dist/assets
browserify build/index.js -o dist/assets/bundle.js
cp favicon.ico dist/
cp *.html dist/
