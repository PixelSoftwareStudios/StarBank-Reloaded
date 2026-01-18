build-mac:
	@mkdir -p dist
	cd dist && fyne package -os darwin -src ../cmd/starbank --release
	@echo "StarBank macOS build complete"