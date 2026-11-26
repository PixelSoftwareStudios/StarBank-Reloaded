dev:
	air

build-mac:
	@mkdir -p dist
	cd dist && fyne package -os darwin -src ../cmd/starbank --release
	@echo "StarBank macOS build complete"

build-windows:
	@mkdir -p dist
	cd dist && fyne package -os windows -src ../cmd/starbank --release
	@echo "StarBank Windows build complete"
