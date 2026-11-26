// Package mapinfo extracts map metadata (name, author, bank names) from
// StarCraft II map archive files (.s2ma). All formats are MPQ archives.
//
// This is used to populate the IsBlizzardMap and AuthorName fields on
// maps.MapItem so the "Hide Blizzard Maps" filter in the sidebar works.
package mapinfo

import (
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"

	"github.com/icza/mpq"
)

// MapRecord holds the author info extracted from a single map file.
type MapRecord struct {
	// Name is the human-readable map name.
	Name string
	// AuthorName is the value from the "// Author:" line in MapScript.galaxy.
	AuthorName string
	// BankNames are the bank names passed to BankLoad() in the trigger code,
	// normalised to lowercase. Used to link a bank file to its parent map.
	BankNames []string
}

// IsBlizzard reports whether this record's author is Blizzard Entertainment.
func (r *MapRecord) IsBlizzard() bool {
	return strings.EqualFold(strings.TrimSpace(r.AuthorName), "Blizzard Entertainment")
}

// Registry maps a bank name (lowercase, without extension) to the MapRecord
// of the map that loads it.
type Registry map[string]*MapRecord

// ScanCache scans all known Blizzard shared directories for .s2ma map archives
// and builds a Registry mapping bank names to map author info.
//
// Returns an empty (non-nil) registry if nothing can be found.
func ScanCache() Registry {
	reg := make(Registry)

	sharedDir := sharedBlizzardDir()
	if sharedDir == "" {
		return reg
	}

	// On macOS: /Users/Shared/Blizzard/StarCraft II/Maps/Cache
	// On Windows: %ProgramData%\Blizzard Entertainment\StarCraft II\Maps\Cache
	scanDir(reg, filepath.Join(sharedDir, "StarCraft II", "Maps", "Cache"))

	// Also scan the Battle.net cache which may hold older .s2ma files.
	scanDir(reg, filepath.Join(sharedDir, "Battle.net", "Cache"))

	return reg
}

// sharedBlizzardDir returns the platform-specific path to the shared Blizzard
// data directory (not the per-user profile directory).
//
//   - macOS:   /Users/Shared/Blizzard
//   - Windows: %ProgramData%\Blizzard Entertainment
func sharedBlizzardDir() string {
	switch runtime.GOOS {
	case "darwin":
		return "/Users/Shared/Blizzard"
	case "windows":
		programData := os.Getenv("ProgramData")
		if programData == "" {
			programData = `C:\ProgramData`
		}
		return filepath.Join(programData, "Blizzard Entertainment")
	default:
		return ""
	}
}

// scanDir walks root recursively and adds a registry entry for each .s2ma file
// that can be parsed.
func scanDir(reg Registry, root string) {
	if root == "" {
		return
	}
	_ = filepath.WalkDir(root, func(path string, d os.DirEntry, walkErr error) error {
		if walkErr != nil || d.IsDir() {
			return nil
		}
		if !strings.EqualFold(filepath.Ext(path), ".s2ma") {
			return nil
		}
		rec := readMapFile(path)
		if rec == nil {
			return nil
		}
		for _, name := range rec.BankNames {
			reg[name] = rec
		}
		return nil
	})
}

// readMapFile opens a .s2ma MPQ archive, reads MapScript.galaxy, and returns a
// parsed MapRecord. Returns nil if the file cannot be read or contains no
// useful data.
func readMapFile(path string) *MapRecord {
	arc, err := mpq.NewFromFile(path)
	if err != nil {
		return nil
	}
	defer arc.Close()

	data, err := arc.FileByName("MapScript.galaxy")
	if err != nil {
		return nil
	}

	rec := parseGalaxyScript(string(data))
	if rec.Name == "" && rec.AuthorName == "" && len(rec.BankNames) == 0 {
		return nil
	}
	return rec
}

var (
	reMapName      = regexp.MustCompile(`(?m)^//\s*Name:\s*(.+)$`)
	reAuthor       = regexp.MustCompile(`(?m)^//\s*Author:\s*(.+)$`)
	reBankLiteral  = regexp.MustCompile(`BankLoad\("([^"]+)"`)
	reBankVariable = regexp.MustCompile(`BankLoad\(([^"]+?)[,\)]`)
)

// parseGalaxyScript extracts name, author, and bank names from galaxy source.
// Variable-name BankLoad calls are resolved with a second-pass assignment search.
func parseGalaxyScript(code string) *MapRecord {
	rec := &MapRecord{}

	if m := reMapName.FindStringSubmatch(code); len(m) > 1 {
		rec.Name = strings.TrimSpace(m[1])
	}
	if m := reAuthor.FindStringSubmatch(code); len(m) > 1 {
		rec.AuthorName = strings.TrimSpace(m[1])
	}

	// 1. String-literal bank names: BankLoad("name")
	for _, m := range reBankLiteral.FindAllStringSubmatch(code, -1) {
		if len(m) > 1 {
			appendUnique(&rec.BankNames, strings.ToLower(strings.TrimSpace(m[1])))
		}
	}

	// 2. Variable bank names: BankLoad(varName)
	//    Try to resolve the variable to its string value by finding
	//    an assignment of the form: varName = "value"
	for _, m := range reBankVariable.FindAllStringSubmatch(code, -1) {
		if len(m) < 2 {
			continue
		}
		varName := regexp.QuoteMeta(strings.TrimSpace(m[1]))
		assignRe := regexp.MustCompile(`\s*` + varName + `\s*=\s*"([^"]+)"`)
		for _, am := range assignRe.FindAllStringSubmatch(code, -1) {
			if len(am) > 1 {
				appendUnique(&rec.BankNames, strings.ToLower(strings.TrimSpace(am[1])))
			}
		}
	}

	return rec
}

func appendUnique(ss *[]string, s string) {
	if s == "" {
		return
	}
	for _, v := range *ss {
		if v == s {
			return
		}
	}
	*ss = append(*ss, s)
}
