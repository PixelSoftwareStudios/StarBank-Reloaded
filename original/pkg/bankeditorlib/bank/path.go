package bank

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// GetSC2RootPath returns the first valid StarCraft II "Documents" root folder.
// On Linux, auto-detection is intentionally not implemented yet.
// Returns os.ErrNotExist if auto-detection is supported but no folder was found.
func GetSC2RootPath() (string, error) {
	switch runtime.GOOS {
	case "windows":
		if p, ok := firstValidWindowsSC2Root(); ok {
			return p, nil
		}
		return "", os.ErrNotExist

	case "darwin":
		if p, ok := firstValidMacSC2Root(); ok {
			return p, nil
		}
		return "", os.ErrNotExist

	case "linux":
		if p, ok := firstValidLinuxSC2Root(); ok {
			return p, nil
		}
		return "", errors.New("linux auto-detection not implemented yet")

	default:
		return "", errors.New("unsupported operating system")
	}
}

// GetBankFolderFromRoot returns all discovered Banks folder(s) for a given SC2 root.
// Because users can have multiple accounts/regions, it returns a slice.
func GetBankFolderFromRoot(sc2Root string) ([]string, error) {
	if strings.TrimSpace(sc2Root) == "" {
		return nil, errors.New("empty sc2Root")
	}
	sc2Root = filepath.Clean(sc2Root)

	// If user passed Banks directly, accept it.
	if looksLikeBanksFolder(sc2Root) {
		if isDir(sc2Root) {
			return []string{sc2Root}, nil
		}
		return nil, errors.New("banks folder does not exist")
	}

	// Normalize Accounts folder if passed.
	accountsDir := ""
	if strings.EqualFold(filepath.Base(sc2Root), "Accounts") && isDir(sc2Root) {
		accountsDir = sc2Root
		sc2Root = filepath.Dir(sc2Root)
	} else {
		accountsDir = filepath.Join(sc2Root, "Accounts")
	}

	if !isValidSC2Root(sc2Root) {
		return nil, errors.New("not a valid StarCraft II root")
	}
	if !isDir(accountsDir) {
		return nil, errors.New("Accounts folder not found under root")
	}

	// Accounts/<acct>/<region>/Banks
	var banks []string
	accts, _ := os.ReadDir(accountsDir)
	for _, a := range accts {
		if !a.IsDir() {
			continue
		}
		acctPath := filepath.Join(accountsDir, a.Name())
		regions, _ := os.ReadDir(acctPath)
		for _, r := range regions {
			if !r.IsDir() {
				continue
			}
			bp := filepath.Join(acctPath, r.Name(), "Banks")
			if isDir(bp) {
				banks = append(banks, bp)
			}
		}
	}

	if len(banks) == 0 {
		return nil, errors.New("no Banks folders found")
	}
	return dedupeClean(banks), nil
}

// IsValidBankPath validates a candidate path that may be one of:
// - a Banks folder
// - a StarCraft II root folder
// - an Accounts folder
//
// It returns ok plus any normalized root/banks paths it can infer.
// Note: Root inference from Banks is best-effort; Banks can be accepted even without a root.
func IsValidBankPath(path string) (ok bool, sc2Root string, banks []string) {
	if strings.TrimSpace(path) == "" {
		return false, "", nil
	}
	p := filepath.Clean(path)

	// Passed Banks directly
	if looksLikeBanksFolder(p) && isDir(p) {
		root := findSC2RootFromBanks(p)
		if root != "" && isValidSC2Root(root) {
			return true, root, []string{p}
		}
		return true, "", []string{p}
	}

	// Passed SC2 root
	if isValidSC2Root(p) {
		b, _ := GetBankFolderFromRoot(p)
		return true, p, b
	}

	// Passed Accounts folder
	if strings.EqualFold(filepath.Base(p), "Accounts") && isDir(p) {
		root := filepath.Dir(p)
		if isValidSC2Root(root) {
			b, _ := GetBankFolderFromRoot(root)
			return true, root, b
		}
	}

	return false, "", nil
}

//
// OS-specific root detection
//

func firstValidWindowsSC2Root() (string, bool) {
	user := os.Getenv("USERPROFILE")
	if user == "" {
		return "", false
	}

	// Common locations:
	cands := []string{
		filepath.Join(user, "Documents", "StarCraft II"),
		filepath.Join(user, "OneDrive", "Documents", "StarCraft II"),
	}

	// OneDrive env vars can be present and more accurate.
	for _, env := range []string{"OneDriveConsumer", "OneDrive"} {
		if od := os.Getenv(env); od != "" {
			cands = append(cands, filepath.Join(od, "Documents", "StarCraft II"))
		}
	}

	for _, p := range dedupeClean(cands) {
		if isValidSC2Root(p) {
			return p, true
		}
	}
	return "", false
}

func firstValidMacSC2Root() (string, bool) {
	home, _ := os.UserHomeDir()
	if home == "" {
		return "", false
	}

	cands := []string{
		// Primary: Battle.net app stores SC2 data here on macOS
		filepath.Join(home, "Library", "Application Support", "Blizzard", "StarCraft II"),
		// Legacy / alternate: older installs or manual Document placements
		filepath.Join(home, "Documents", "StarCraft II"),
		// iCloud Drive — Documents may be redirected here when iCloud is on
		filepath.Join(home, "Library", "Mobile Documents", "com~apple~CloudDocs", "Documents", "StarCraft II"),
	}

	for _, p := range cands {
		if isValidSC2Root(p) {
			return p, true
		}
	}
	return "", false
}

func firstValidLinuxSC2Root() (string, bool) {
	// TODO: implement Proton / Lutris scanning in the future.
	// For now, auto-detection on Linux is intentionally disabled.
	return "", false
}

//
// Internals
//

func isValidSC2Root(path string) bool {
	if !isDir(path) {
		return false
	}
	// Most reliable marker is Accounts dir.
	if isDir(filepath.Join(path, "Accounts")) {
		return true
	}
	// Fallback markers:
	if isFile(filepath.Join(path, "ExecuteInfo.txt")) || isFile(filepath.Join(path, "Variables.txt")) {
		return true
	}
	return false
}

func looksLikeBanksFolder(path string) bool {
	return strings.EqualFold(filepath.Base(path), "Banks")
}

func findSC2RootFromBanks(banksPath string) string {
	// BanksPath: .../StarCraft II/Accounts/<acct>/<region>/Banks
	// Root is 4 levels up from Banks: Banks -> <region> -> <acct> -> Accounts -> root
	p := filepath.Clean(banksPath)
	for i := 0; i < 4; i++ {
		p = filepath.Dir(p)
	}
	return p
}

func isDir(path string) bool {
	st, err := os.Stat(path)
	return err == nil && st.IsDir()
}

func isFile(path string) bool {
	st, err := os.Stat(path)
	return err == nil && !st.IsDir()
}

func dedupeClean(in []string) []string {
	seen := map[string]struct{}{}
	out := make([]string, 0, len(in))
	for _, p := range in {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}
		cp := filepath.Clean(p)
		if _, ok := seen[cp]; ok {
			continue
		}
		seen[cp] = struct{}{}
		out = append(out, cp)
	}
	return out
}
