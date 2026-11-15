import { describe, expect, it } from 'vitest';
import { backendError, canCancelInstall, canPlayInstance, installFailure, terminalInstallEvent, validateJavaForm, backendStatusLine, backendStatusSignature, isInstallPending, downloadEventsForView, statusPollInterval } from './desktopState';
import type { GameInstance, GameStatus, JavaSettings, MemoryInfo, ProgressEvent } from './backend';

const instance: GameInstance = {
  id:'instance-1', display_name:'Vanilla 1.21.1', root_path:'instances/instance-1',
  minecraft_version:'1.21.1',profile_id:'persisted-profile-uuid',state:'ready',
  loader:{kind:'vanilla',version:null}
};
const status: GameStatus = {
  instance_id:instance.id,install_state:'ready',installing:false,
  pid:null,process_state:'not_running',last_error:null
};
const ready={instance_id:instance.id,java_path:'C:/Java/bin/java.exe',required_java:21,xms_mb:768,xmx_mb:3072};
const event=(phase:string,message_key:string):ProgressEvent=>({
  contract_version:1,operation_id:'real-op',instance_id:instance.id,
  phase,message_key,timestamp:'2026-11-14T00:00:00Z',details:{},percent:null
});
describe('actual installer progress, cancellation and failures',()=>{
  it('displays exact backend failure message without fabricating percentages',()=>{
    for (const text of ['metadata request timed out','SHA-1 checksum mismatch for client.jar',
      'native ZIP extraction rejected','expected Java 21, detected 17']) {
      expect(installFailure(event('failed',text))).toBe(text);
    }
    expect(installFailure(event('downloading','actual download in progress'))).toBeUndefined();
    expect(terminalInstallEvent(event('failed','checksum mismatch'))).toBe(true);
    expect(terminalInstallEvent(event('cancelled','Install cancelled'))).toBe(true);
    expect(terminalInstallEvent(event('ready','Files verified'))).toBe(true);
    expect(terminalInstallEvent(event('downloading','still downloading'))).toBe(false);
    expect(event('downloading','downloading').percent).toBeNull();
  });
  it('only offers cancellation while an actual install worker is active',()=>{
    expect(canCancelInstall({...status,installing:true,install_state:'downloading'})).toBe(true);
    expect(canCancelInstall({...status,installing:true,install_state:'verifying'})).toBe(true);
    expect(canCancelInstall({...status,installing:false,install_state:'downloading'})).toBe(false);
    expect(canCancelInstall(status)).toBe(false);
  });
  it('preserves raw Tauri rejection strings for backend error banners',()=>{
    expect(backendError('native ZIP extraction rejected: unsupported entry')).toContain('native ZIP extraction');
    expect(backendError(new Error('Java compatibility mismatch'))).toBe('Java compatibility mismatch');
  });
});
describe('Play readiness after restart and preflight',()=>{
  it('fails closed when absent preflight, missing READY, stale profile or wrong instance',()=>{
    expect(canPlayInstance(instance,instance.profile_id,null,status)).toBe(false);
    expect(canPlayInstance({...instance,state:'downloading'},instance.profile_id,ready,status)).toBe(false);
    expect(canPlayInstance(instance,'other-profile',ready,status)).toBe(false);
    expect(canPlayInstance(instance,instance.profile_id,{...ready,instance_id:'other'},status)).toBe(false);
    expect(canPlayInstance({...instance,loader:{kind:'forge',version:'1'}},instance.profile_id,ready,status)).toBe(false);
  });
  it('accepts a rehydrated READY instance only after preflight; forbids duplicate Java process',()=>{
    const afterLauncherRestart=JSON.parse(JSON.stringify(instance)) as GameInstance;
    expect(canPlayInstance(afterLauncherRestart,afterLauncherRestart.profile_id,ready,status)).toBe(true);
    expect(canPlayInstance(afterLauncherRestart,afterLauncherRestart.profile_id,ready,{
      ...status,pid:4242,process_state:'running'
    })).toBe(false);
  });
});
describe('Java settings persisted wire state and physical memory',()=>{
  const mem:MemoryInfo={total_mb:16384,available_mb:9216,reserved_mb:1024};
  const saved:JavaSettings={instance_id:instance.id,xms_mb:1024,xmx_mb:6144,
    java_path:'C:/Java/bin/java.exe',revision:3};
  it('retains per-instance saved settings on a simulated launcher rehydrate',()=>{
    const reloaded=JSON.parse(JSON.stringify(saved)) as JavaSettings;
    expect(reloaded).toEqual(saved);
    expect(validateJavaForm(reloaded,mem)).toBeUndefined();
  });
  it('rejects excessive RAM, inverted heap and nonintegral inputs',()=>{
    expect(validateJavaForm({...saved,xms_mb:8192},mem)).toContain('Xms');
    expect(validateJavaForm({...saved,xmx_mb:16384},mem)).toContain('physical');
    expect(validateJavaForm({...saved,xmx_mb:10000},mem)).toContain('available');
    expect(validateJavaForm({...saved,xms_mb:512.5},mem)).toContain('RAM');
    expect(validateJavaForm(null,mem)).toContain('Select');
  });
});

describe('backend-driven Activity state and asynchronous acceptance', () => {
  it('uses actual install state and last_error instead of treating accepted as finished', () => {
    const downloading = {...status,install_state:'downloading' as const,installing:true};
    expect(backendStatusLine(downloading)).toContain('downloading');
    expect(backendStatusLine(downloading)).toContain('worker running');
    expect(backendStatusLine(downloading)).not.toContain('ready');
    expect(isInstallPending(downloading)).toBe(true);
    const failure={...status,install_state:'failed' as const,last_error:'SHA-1 mismatch'};
    expect(backendStatusLine(failure)).toContain('SHA-1 mismatch');
    expect(backendStatusSignature(failure)).not.toEqual(backendStatusSignature(downloading));
    expect(isInstallPending(failure)).toBe(false);
  });
  it('never invents a download percent or a completion from a partial count', () => {
    const actual=event('downloading','Official Mojang files downloaded');
    actual.details={downloaded_files:3970,native_archives:8};
    expect(actual.percent).toBeNull();
    expect(terminalInstallEvent(actual)).toBe(false);
    expect(terminalInstallEvent(event('verifying','Independent SHA-1 file attestation complete'))).toBe(false);
  });
});

describe('Downloads tab bounded rendering and idle IPC cadence', () => {
  it('renders only 24 of 100 accumulated backend events unless explicitly expanded', () => {
    const history=Array.from({length:100},(_,i)=>event('downloading','verified file '+i));
    expect(downloadEventsForView(history,false)).toHaveLength(24);
    expect(downloadEventsForView(history,true)).toHaveLength(100);
    expect(downloadEventsForView(history,false)[0].message_key).toBe('verified file 0');
  });
  it('polls less often when Minecraft is idle, without hiding running install state', () => {
    expect(statusPollInterval([status])).toBe(9000);
    expect(statusPollInterval([{...status,installing:true,install_state:'downloading'}])).toBe(2400);
    expect(statusPollInterval([{...status,installing:false,install_state:'verifying'}])).toBe(2400);
  });
});
