import AppKit
import ApplicationServices
import Darwin
import Foundation

struct PerfError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
func since(_ start: UInt64) -> Double { Double(now() - start) / 1e6 }

func attr<T>(_ e: AXUIElement, _ name: String) -> T? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(e, name as CFString, &value) == .success else {
        return nil
    }
    return value as? T
}
func labels(_ e: AXUIElement) -> [String] {
    ["AXTitle", "AXDescription", "AXValue"].compactMap { attr(e, $0) }
}
func find(_ elements: [AXUIElement], depth: Int = 0, where matches: (AXUIElement) -> Bool) -> AXUIElement? {
    guard depth < 15 else { return nil }
    for element in elements {
        if matches(element) { return element }
        let children: [AXUIElement] = attr(element, "AXChildren") ?? []
        if let descendant = find(children, depth: depth + 1, where: matches) { return descendant }
    }
    return nil
}
func visibleWindow(_ pid: pid_t) -> Bool {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return false }
    return list.contains {
        ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid &&
        ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
    }
}
func resources(_ pid: pid_t) throws -> rusage_info_v4 {
    var usage = rusage_info_v4()
    let status = withUnsafeMutablePointer(to: &usage) {
        $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) {
            proc_pid_rusage(pid, RUSAGE_INFO_V4, $0)
        }
    }
    guard status == 0 else { throw PerfError("proc_pid_rusage failed for trial host") }
    return usage
}
func directChildNames(_ pid: pid_t) throws -> [String] {
    let command = Process()
    command.executableURL = URL(fileURLWithPath: "/bin/ps")
    command.arguments = ["-axo", "ppid=,comm="]
    let output = Pipe()
    command.standardOutput = output
    try command.run()
    let data = output.fileHandleForReading.readDataToEndOfFile()
    command.waitUntilExit()
    guard command.terminationStatus == 0 else { throw PerfError("ps failed") }
    return String(decoding: data, as: UTF8.self).split(separator: "\n").compactMap { line in
        let fields = line.split(maxSplits: 1, whereSeparator: \.isWhitespace)
        guard fields.count == 2, pid_t(fields[0]) == pid else { return nil }
        return URL(fileURLWithPath: String(fields[1])).lastPathComponent
    }.sorted()
}

let files = FileManager.default
let binary = URL(fileURLWithPath: CommandLine.arguments[1])
let folder = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
let repeats = Int(CommandLine.arguments[3])!
let jobs: [(String, Int)] = [("empty", 0), ("small", 32), ("large", 10_000)]

func trial(_ name: String, values: Int, iteration: Int) throws -> [String: Any] {
    let home = folder.appendingPathComponent("\(name)-\(iteration)", isDirectory: true)
    for path in ["Config", "Inventory", "Archives"] {
        try files.createDirectory(at: home.appendingPathComponent(path, isDirectory: true), withIntermediateDirectories: true)
    }
    let bank = home.appendingPathComponent("Synthetic.SC2Bank")
    if values > 0 {
        let keys = (0..<values).map { "<Key name=\"Value\(String(format: "%05d", $0))\"><Value int=\"\($0)\"/></Key>" }.joined(separator: "\n")
        try "<Bank version=\"1\"><Section name=\"PerfProbe\">\n\(keys)\n</Section></Bank>\n".write(to: bank, atomically: true, encoding: .utf8)
    }
    let process = Process()
    process.executableURL = binary
    process.currentDirectoryURL = home
    process.arguments = values > 0 ? [bank.path] : []
    var environment = ProcessInfo.processInfo.environment
    environment["STARBANK_BENCH_ROOT"] = home.path
    process.environment = environment
    let log = home.appendingPathComponent("app.log")
    guard files.createFile(atPath: log.path, contents: nil) else { throw PerfError("cannot create scratch log") }
    let stream = try FileHandle(forWritingTo: log)
    process.standardOutput = stream
    process.standardError = stream

    let start = now()
    try process.run()
    let pid = process.processIdentifier
    defer {
        if process.isRunning {
            process.terminate()
            for _ in 0..<50 where process.isRunning { Thread.sleep(forTimeInterval: 0.02) }
            if process.isRunning { kill(pid, SIGKILL) }
        }
        process.waitUntilExit()
    }
    let app = AXUIElementCreateApplication(pid)
    AXUIElementSetMessagingTimeout(app, 0.25)
    var windowMs: Double?
    var readyMs: Double?
    var openButton: AXUIElement?
    let expected = values > 0 ? "Open" : "Open Bank…"
    while process.isRunning && since(start) < 9000 && (windowMs == nil || readyMs == nil) {
        if windowMs == nil && visibleWindow(pid) { windowMs = since(start) }
        if windowMs != nil && readyMs == nil {
            let windows: [AXUIElement] = attr(app, "AXWindows") ?? []
            openButton = find(windows) {
                (attr($0, "AXRole") as String?) == "AXButton" &&
                (attr($0, "AXEnabled") as Bool? ?? false) &&
                labels($0).contains(expected)
            }
            if openButton != nil { readyMs = since(start) }
        }
        Thread.sleep(forTimeInterval: 0.025)
    }
    guard let windowMs, let readyMs, let openButton else {
        throw PerfError("\(name) \(iteration) failed on window/UI setup (window: \(String(describing: windowMs)), ready: \(String(describing: readyMs)), status: \(process.isRunning))")
    }
    var openMs: Double?
    if values > 0 {
        let opened = now()
        guard AXUIElementPerformAction(openButton, "AXPress" as CFString) == .success else { throw PerfError("startup confirmation could not be pressed") }
        while process.isRunning && since(opened) < 9000 {
            let windows: [AXUIElement] = attr(app, "AXWindows") ?? []
            if find(windows, where: { labels($0).contains("PerfProbe") }) != nil {
                openMs = since(opened)
                break
            }
            Thread.sleep(forTimeInterval: 0.025)
        }
        guard openMs != nil else { throw PerfError("\(name) \(iteration) did not render the synthetic section") }
    }
    Thread.sleep(forTimeInterval: 1.0)
    var children = Set(try directChildNames(pid))
    let beginning = try resources(pid)
    let idleStart = now()
    var samples: [[String: UInt64]] = []
    for index in 0..<7 {
        if index != 0 { Thread.sleep(forTimeInterval: 0.5) }
        let usage = try resources(pid)
        samples.append(["resident_bytes": usage.ri_resident_size, "physical_footprint_bytes": usage.ri_phys_footprint])
    }
    let idleMs = since(idleStart)
    let ending = try resources(pid)
    children.formUnion(try directChildNames(pid))
    var data: [String: Any] = [
        "case": name, "warmup": iteration == 0, "iteration": iteration,
        "values": values, "file_bytes": values > 0 ? (try bank.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? -1) : 0,
        "app_pid": pid, "window_ms": windowMs, "ui_ms": readyMs,
        "idle_interval_ms": idleMs, "app_samples": samples, "app_child_names": children.sorted(),
        "app_idle_user_ticks": ending.ri_user_time - beginning.ri_user_time,
        "app_idle_system_ticks": ending.ri_system_time - beginning.ri_system_time,
        "app_lifetime_max_physical_footprint_bytes": ending.ri_lifetime_max_phys_footprint
    ]
    if let openMs { data["open_ms"] = openMs }
    return data
}

do {
    guard AXIsProcessTrusted() else { throw PerfError("runner does not have macOS Accessibility access") }
    for iteration in 0...repeats {
        for (name, values) in jobs {
            let result = try trial(name, values: values, iteration: iteration)
            let json = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
            print(String(decoding: json, as: UTF8.self))
            fflush(stdout)
        }
    }
} catch {
    fputs("\(error)\n", stderr)
    exit(1)
}
