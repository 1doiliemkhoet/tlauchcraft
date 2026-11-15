// Windows-only process containment helper. Used INSIDE a network-disabled Windows Sandbox.
// Must launch suspended, assign Job before ResumeThread; closes Job to kill descendants.
// This helper cannot by itself enforce filesystem or network isolation.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class ForgeJobProcess
{
    const uint CREATE_SUSPENDED = 0x00000004;
    const uint CREATE_NO_WINDOW = 0x08000000;
    const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
    const uint JOB_OBJECT_LIMIT_ACTIVE_PROCESS = 0x00000008;
    const uint WAIT_OBJECT_0 = 0;
    const uint WAIT_TIMEOUT = 258;

    [StructLayout(LayoutKind.Sequential)]
    struct STARTUPINFO {
        public uint cb;
        public string lpReserved;
        public string lpDesktop;
        public string lpTitle;
        public uint dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars;
        public uint dwFillAttribute, dwFlags;
        public short wShowWindow, cbReserved2;
        public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct PROCESS_INFORMATION {
        public IntPtr hProcess, hThread;
        public uint dwProcessId, dwThreadId;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct IO_COUNTERS {
        public ulong ReadOperationCount, WriteOperationCount, OtherOperationCount;
        public ulong ReadTransferCount, WriteTransferCount, OtherTransferCount;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        public long PerProcessUserTimeLimit, PerJobUserTimeLimit;
        public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize, MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public UIntPtr Affinity;
        public uint PriorityClass, SchedulingClass;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
        public IO_COUNTERS IoInfo;
        public UIntPtr ProcessMemoryLimit, JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed, PeakJobMemoryUsed;
    }
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool SetInformationJobObject(IntPtr job, int infoClass, IntPtr info, uint length);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool CreateProcessW(string app, StringBuilder commandLine, IntPtr procAttributes,
        IntPtr threadAttributes, bool inheritHandles, uint flags, IntPtr environment,
        string currentDirectory, ref STARTUPINFO startup, out PROCESS_INFORMATION process);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool TerminateProcess(IntPtr handle, uint exitCode);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool CloseHandle(IntPtr handle);

    static string Quote(string value) {
        if (value.IndexOf('\0') >= 0) throw new ArgumentException("NUL in argument");
        if (value.Length == 0) return "\"\"";
        if (value.IndexOfAny(new char[] {' ', '\t', '\n', '"'}) < 0) return value;
        var b = new StringBuilder("\"");
        int slashes = 0;
        foreach (char ch in value) {
            if (ch == '\\') { slashes++; continue; }
            if (ch == '"') {
                b.Append('\\', slashes * 2 + 1);
                b.Append('"');
                slashes = 0;
                continue;
            }
            b.Append('\\', slashes);
            slashes = 0;
            b.Append(ch);
        }
        b.Append('\\', slashes * 2);
        b.Append('"');
        return b.ToString();
    }

    public static int Run(string executable, string[] arguments, string workDir, int timeoutMillis) {
        if (string.IsNullOrWhiteSpace(executable) || !System.IO.Path.IsPathRooted(executable)
            || !System.IO.File.Exists(executable) || !System.IO.Directory.Exists(workDir)
            || timeoutMillis <= 0 || timeoutMillis > 900000)
            throw new ArgumentException("Untrusted executable path, directory or timeout");
        IntPtr job = IntPtr.Zero, structMem = IntPtr.Zero;
        PROCESS_INFORMATION pi = new PROCESS_INFORMATION();
        bool started = false, assigned = false;
        try {
            job = CreateJobObject(IntPtr.Zero, null);
            if (job == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "Job creation failed");
            var limits = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
            limits.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            limits.BasicLimitInformation.ActiveProcessLimit = 16;
            uint size = (uint)Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
            structMem = Marshal.AllocHGlobal((int)size);
            Marshal.StructureToPtr(limits, structMem, false);
            if (!SetInformationJobObject(job, 9, structMem, size))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Job limits rejected");
            var cmd = new StringBuilder(Quote(executable));
            foreach (string arg in arguments) cmd.Append(' ').Append(Quote(arg));
            var startup = new STARTUPINFO();
            startup.cb = (uint)Marshal.SizeOf(typeof(STARTUPINFO));
            if (!CreateProcessW(executable, cmd, IntPtr.Zero, IntPtr.Zero, false,
                CREATE_SUSPENDED | CREATE_NO_WINDOW, IntPtr.Zero, workDir, ref startup, out pi))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Suspended process creation failed");
            started = true;
            if (!AssignProcessToJobObject(job, pi.hProcess))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not assign child to Job");
            assigned = true;
            if (ResumeThread(pi.hThread) == uint.MaxValue)
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Resume denied");
            uint wait = WaitForSingleObject(pi.hProcess, (uint)timeoutMillis);
            if (wait != WAIT_OBJECT_0) {
                if (wait == WAIT_TIMEOUT) throw new TimeoutException("Processor exceeded approved timeout");
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Wait failed");
            }
            uint exitCode;
            if (!GetExitCodeProcess(pi.hProcess, out exitCode))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Exit status unavailable");
            return unchecked((int)exitCode);
        } finally {
            // Closing the Job kills any living descendants, even on timeout/errors.
            if (started && !assigned) TerminateProcess(pi.hProcess, 1);
            if (job != IntPtr.Zero) CloseHandle(job);
            if (pi.hThread != IntPtr.Zero) CloseHandle(pi.hThread);
            if (pi.hProcess != IntPtr.Zero) CloseHandle(pi.hProcess);
            if (structMem != IntPtr.Zero) Marshal.FreeHGlobal(structMem);
        }
    }
}
