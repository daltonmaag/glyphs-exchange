cargo install cargo-wix
cargo wix --package glyphs-exchange

# Need to do this, or powershell just pretends all is fine when the last process
# complained?! See https://github.com/PowerShell/PowerShell/issues/11712.
exit $LASTEXITCODE
