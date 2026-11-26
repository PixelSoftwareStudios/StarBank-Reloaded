import Darwin
import Foundation

func procCPU() -> UInt64 {
    var usage = rusage_info_v4()
    let status = withUnsafeMutablePointer(to: &usage) {
        $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) {
            proc_pid_rusage(getpid(), RUSAGE_INFO_V4, $0)
        }
    }
    precondition(status == 0)
    return usage.ri_user_time + usage.ri_system_time
}
func portableCPU() -> Int64 {
    var usage = rusage()
    precondition(getrusage(RUSAGE_SELF, &usage) == 0)
    return Int64(usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) * 1_000_000 + Int64(usage.ru_utime.tv_usec + usage.ru_stime.tv_usec)
}
var base = mach_timebase_info_data_t()
precondition(mach_timebase_info(&base) == KERN_SUCCESS)
let start = DispatchTime.now().uptimeNanoseconds
let beforeMach = procCPU()
let beforePortable = portableCPU()
var cycles = 0
while DispatchTime.now().uptimeNanoseconds - start < 250_000_000 {
    cycles += 1
}
let deltaMach = procCPU() - beforeMach
let deltaPortable = portableCPU() - beforePortable
print("timebase \(base.numer)/\(base.denom) ns/tick, proc_pid_rusage delta \(deltaMach) ticks, getrusage delta \(deltaPortable) us, loop iterations \(cycles)")
