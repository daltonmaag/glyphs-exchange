fn main() {
    // Sets version & description metadata for Windows executables
    #[cfg(windows)]
    {
        let res = winres::WindowsResource::new();
        res.compile().unwrap();
    }
}
