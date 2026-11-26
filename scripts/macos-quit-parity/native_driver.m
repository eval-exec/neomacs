// Test-only injection into one harness-owned editor. Build with ARC/blocks and
// AppKit; never load into an unrelated application. No keyboard events, process
// signals, delegate replacement, or changes to the original termination reply.
// GNU 31.1 nsterm.m:6096,6588,6628 separates menu terminate: from delegate consent.
#import <AppKit/AppKit.h>
#import <dispatch/dispatch.h>
#import <objc/runtime.h>
#include <dlfcn.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>

@class NeomacsQuitNativeDriver;
static NeomacsQuitNativeDriver *driver;
static id countedDelegate;
static Class countedClass;
typedef NSApplicationTerminateReply (*TerminationMethod)(id, SEL, NSApplication *);
static TerminationMethod originalTermination;
static _Atomic unsigned long delegateCalls;
static NSApplicationTerminateReply countTermination(id receiver, SEL selector, NSApplication *application);

@interface NeomacsQuitNativeDriver : NSObject
@property(nonatomic, copy) NSString *controlPath;
@property(nonatomic, copy) NSString *resultPath;
@property(nonatomic, strong) NSMutableDictionary *state;
@property(nonatomic, strong) NSTimer *pollTimer;
@property(nonatomic, strong) NSTimer *alertTimer;
@property(nonatomic) NSTimeInterval readinessDeadline;
@property(nonatomic) NSTimeInterval controlDeadline;
@property(nonatomic) NSTimeInterval alertDeadline;
@property(nonatomic) BOOL ready;
@property(nonatomic) BOOL finished;
@property(nonatomic) BOOL clicked;
@property(nonatomic, copy) NSString *command;
- (void)writeState;
- (void)start;
- (void)finish:(NSString *)error;
- (BOOL)witnessCurrent;
- (BOOL)installWitness;
- (void)poll:(NSTimer *)timer;
- (void)watchAlert:(NSTimer *)timer;
- (void)invoke;
@end

static NSTimeInterval now(void) {
    return NSProcessInfo.processInfo.systemUptime;
}

static NSMenuItem *quitItem(void) {
    NSMenu *main = NSApp.mainMenu;
    if (main.numberOfItems == 0) return nil;
    NSMenu *application = [main itemAtIndex:0].submenu;
    for (NSMenuItem *item in application.itemArray) {
        if (!item.action) continue;
        if (!item.action) continue;
        NSString *action = NSStringFromSelector(item.action);
        if ([action isEqualToString:@"terminate:"] ||
            [action isEqualToString:@"neomacsQuit:"]) return item;
    }
    return nil;
}

static NSButton *buttonNamed(NSView *view, NSString *title) {
    if ([view isKindOfClass:NSButton.class] &&
        [((NSButton *)view).title isEqualToString:title]) return (NSButton *)view;
    for (NSView *child in view.subviews) {
        NSButton *button = buttonNamed(child, title);
        if (button) return button;
    }
    return nil;
}

static NSString *replyName(NSInteger reply) {
    switch (reply) {
        case NSTerminateCancel: return @"cancel";
        case NSTerminateNow: return @"now";
        case NSTerminateLater: return @"later";
        default: return @"unknown";
    }
}

@implementation NeomacsQuitNativeDriver
- (void)writeState {
    self.state[@"delegate_calls"] = @(atomic_load(&delegateCalls));
    NSError *error = nil;
    NSData *json = [NSJSONSerialization dataWithJSONObject:self.state
        options:NSJSONWritingPrettyPrinted error:&error];
    if (!json || ![json writeToFile:self.resultPath options:NSDataWritingAtomic error:&error])
        fprintf(stderr, "native quit driver result write failed: %s\n",
            error.description.UTF8String ?: "unknown error");
}

- (void)finish:(NSString *)error {
    self.finished = YES;
    [self.pollTimer invalidate];
    [self.alertTimer invalidate];
    self.state[@"status"] = error ? @"error" : @"done";
    if (error) self.state[@"error"] = error;
    [self writeState];
}

- (BOOL)witnessCurrent {
    Method method = class_getInstanceMethod(countedClass, @selector(applicationShouldTerminate:));
    return NSApp.delegate == countedDelegate && object_getClass(countedDelegate) == countedClass
        && method && method_getImplementation(method) == (IMP)countTermination;
}

- (BOOL)installWitness {
    id delegate = NSApp.delegate;
    Class cls = object_getClass(delegate);
    SEL selector = @selector(applicationShouldTerminate:);
    Method inherited = class_getInstanceMethod(cls, selector);
    if (!inherited) return NO;
    // If inherited, isolate the override onto the actual installed class;
    // never mutate a superclass shared by other application objects.
    class_addMethod(cls, selector, method_getImplementation(inherited), method_getTypeEncoding(inherited));
    Method method = class_getInstanceMethod(cls, selector);
    IMP expected = method_getImplementation(method);
    if (expected == (IMP)countTermination) return NO;
    countedDelegate = delegate;
    countedClass = cls;
    originalTermination = (TerminationMethod)expected;
    IMP previous = method_setImplementation(method, (IMP)countTermination);
    if (previous != expected) {
        originalTermination = (TerminationMethod)previous;
        method_setImplementation(method, previous);
        return NO;
    }
    self.state[@"delegate_original_imp"] = [NSString stringWithFormat:@"%p", (void *)expected];
    self.state[@"delegate_wrapper_imp"] = [NSString stringWithFormat:@"%p", (void *)countTermination];
    self.state[@"witness_installed"] = @YES;
    return [self witnessCurrent];
}

- (void)start {
    self.state[@"main_thread"] = @([NSThread isMainThread]);
    self.state[@"status"] = @"waiting";
    self.readinessDeadline = now() + 20.0;
    [self writeState];
    self.pollTimer = [NSTimer timerWithTimeInterval:0.05 target:self
        selector:@selector(poll:) userInfo:nil repeats:YES];
    NSRunLoop *loop = NSRunLoop.mainRunLoop;
    [loop addTimer:self.pollTimer forMode:NSRunLoopCommonModes];
    [loop addTimer:self.pollTimer forMode:NSModalPanelRunLoopMode];
}

- (void)poll:(NSTimer *)timer {
    if (self.finished) return;
    if (!self.ready) {
        NSDictionary *before = [self.state copy];
        id delegate = NSApp.delegate;
        NSMenuItem *item = quitItem();
        self.state[@"application_ready"] = @(NSApp != nil);
        self.state[@"delegate_ready"] = @([delegate respondsToSelector:@selector(applicationShouldTerminate:)]);
        self.state[@"menu_ready"] = @(item != nil);
        self.state[@"delegate_class"] = delegate ? NSStringFromClass(object_getClass(delegate)) : @"";
        self.state[@"menu_title"] = item.title ?: @"";
        self.state[@"menu_action"] = item ? NSStringFromSelector(item.action) : @"";
        if (NSApp && [self.state[@"delegate_ready"] boolValue] && item) {
            if (![self installWitness]) {
                [self finish:@"could not install/verify actual delegate count witness"];
                return;
            }
            self.ready = YES;
            self.controlDeadline = now() + 20.0;
            self.state[@"status"] = @"ready";
        } else if (now() >= self.readinessDeadline) {
            [self finish:@"native application/delegate/Quit menu did not become ready"];
            return;
        }
        if (![before isEqualToDictionary:self.state]) [self writeState];
        return;
    }
    if (now() >= self.controlDeadline) {
        [self finish:@"no command received before control deadline"];
        return;
    }
    if (![NSFileManager.defaultManager fileExistsAtPath:self.controlPath]) return;
    NSError *error = nil;
    NSDictionary *attributes = [NSFileManager.defaultManager attributesOfItemAtPath:self.controlPath error:&error];
    if (!attributes || [attributes fileSize] > 256) {
        [self finish:@"control file unavailable or larger than 256 bytes"];
        return;
    }
    NSString *text = [NSString stringWithContentsOfFile:self.controlPath encoding:NSUTF8StringEncoding error:&error];
    if (!text) {
        [self finish:@"control file is not readable UTF-8"];
        return;
    }
    self.command = [text stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    [timer invalidate]; // Exactly one command per injected editor.
    if (![self witnessCurrent] || !quitItem()) {
        [self finish:@"native delegate/class/implementation or Quit menu changed after readiness"];
        return;
    }
    self.state[@"command"] = self.command;
    if ([self.command isEqualToString:@"snapshot"]) {
        [self finish:nil];
        return;
    }
    [self invoke];
}

- (void)watchAlert:(NSTimer *)timer {
    if (self.finished) return;
    NSWindow *window = NSApp.modalWindow;
    NSButton *accept = buttonNamed(window.contentView, @"Save Buffers and Exit");
    NSButton *cancel = buttonNamed(window.contentView, @"Cancel");
    if (window && accept && cancel) {
        self.state[@"alert_seen"] = @YES;
        self.state[@"alert_window_class"] = NSStringFromClass(object_getClass(window));
        BOOL accepting = [self.command isEqualToString:@"delegate-accept"];
        BOOL cancelling = [self.command isEqualToString:@"delegate-cancel"];
        if (accepting || cancelling) {
            NSButton *button = accepting ? accept : cancel;
            self.clicked = YES;
            self.state[@"button_title"] = button.title;
            [timer invalidate];
            [self writeState];
            [button performClick:nil];
            return;
        }
    }
    if (now() >= self.alertDeadline) {
        self.state[@"modal_aborted"] = @(window != nil);
        [self finish:@"native callback/expected alert did not complete before deadline"];
        // No modal was present before invoke. This is only this owned child
        // process's modal loop; never send events/signals to another app.
        if (window) [NSApp abortModal];
    }
}

- (void)invoke {
    BOOL menu = [self.command isEqualToString:@"menu"];
    BOOL terminate = [self.command isEqualToString:@"terminate"];
    BOOL delegate = [@[@"delegate-nil", @"delegate-accept", @"delegate-cancel"] containsObject:self.command];
    if (!menu && !terminate && !delegate) {
        [self finish:@"unknown command"];
        return;
    }
    if (NSApp.modalWindow) {
        [self finish:@"refusing to interfere with a pre-existing modal window"];
        return;
    }
    if (menu && ![quitItem() isEnabled]) {
        [self finish:@"actual Quit menu item is disabled"];
        return;
    }
    NSEvent *event = NSApp.currentEvent;
    BOOL commandKey = event && event.type == NSEventTypeKeyDown
        && (event.modifierFlags & NSEventModifierFlagCommand) != 0;
    self.state[@"current_event_type"] = event ? @(event.type) : (id)NSNull.null;
    self.state[@"current_command_key_down"] = @(commandKey);
    self.state[@"synthetic_key_events"] = @NO;
    if (menu && commandKey) {
        [self finish:@"Quit menu dispatch has a current Command key event; route is ambiguous"];
        return;
    }
    self.state[@"status"] = @"invoking";
    [self writeState]; // Survives an actual terminate: that never returns.
    self.alertDeadline = now() + 8.0;
    self.alertTimer = [NSTimer timerWithTimeInterval:0.05 target:self
        selector:@selector(watchAlert:) userInfo:nil repeats:YES];
    [NSRunLoop.mainRunLoop addTimer:self.alertTimer forMode:NSRunLoopCommonModes];
    [NSRunLoop.mainRunLoop addTimer:self.alertTimer forMode:NSModalPanelRunLoopMode];
    @try {
        if (menu) {
            NSMenuItem *item = quitItem();
            BOOL sent = [NSApp sendAction:item.action to:item.target from:item];
            self.state[@"action_sent"] = @(sent);
            if (!sent) { [self finish:@"native Quit menu action was not sent"]; return; }
        } else if (terminate) {
            [NSApp terminate:nil];
            self.state[@"terminate_returned"] = @YES;
        } else {
            NSInteger reply = (NSInteger)[(id<NSApplicationDelegate>)countedDelegate applicationShouldTerminate:NSApp];
            self.state[@"native_reply"] = @(reply);
            self.state[@"native_reply_name"] = replyName(reply);
        }
    } @catch (NSException *exception) {
        [self finish:[NSString stringWithFormat:@"native callback raised %@: %@", exception.name, exception.reason]];
        return;
    }
    if (self.finished) { [self writeState]; return; }
    if (([self.command isEqualToString:@"delegate-accept"] ||
         [self.command isEqualToString:@"delegate-cancel"]) && !self.clicked) {
        [self finish:@"delegate returned without the expected native confirmation alert"];
        return;
    }
    [self finish:nil];
}
@end

// Preserve the SDK's exact method ABI. Count only this installed delegate and
// tail-call its saved implementation without replacing its reply or flow.
static NSApplicationTerminateReply countTermination(id receiver, SEL selector, NSApplication *application) {
    BOOL counted = receiver == countedDelegate;
    if (counted) {
        atomic_fetch_add(&delegateCalls, 1);
        if (NSThread.isMainThread) [driver writeState];
    }
    NSApplicationTerminateReply reply = originalTermination(receiver, selector, application);
    if (counted && NSThread.isMainThread) {
        driver.state[@"native_reply"] = @(reply);
        driver.state[@"native_reply_name"] = replyName(reply);
        driver.state[@"last_delegate_reply"] = @(reply);
        driver.state[@"last_delegate_reply_name"] = replyName(reply);
        [driver writeState];
    }
    return reply;
}

__attribute__((constructor)) static void loadNativeQuitDriver(void) {
    const char *control = getenv("NEOMACS_QUIT_NATIVE_CONTROL");
    const char *result = getenv("NEOMACS_QUIT_NATIVE_RESULT");
    if (!control || !result) return;
    @autoreleasepool {
        NSString *controlPath = [NSString stringWithUTF8String:control];
        NSString *resultPath = [NSString stringWithUTF8String:result];
        if (!controlPath.isAbsolutePath || !resultPath.isAbsolutePath || [controlPath isEqualToString:resultPath]) {
            fprintf(stderr, "native quit driver requires distinct absolute owned control/result paths\n");
            return;
        }
        Dl_info image = {0};
        dladdr((const void *)loadNativeQuitDriver, &image);
        driver = [NeomacsQuitNativeDriver new];
        driver.controlPath = controlPath;
        driver.resultPath = resultPath;
        driver.state = [@{@"status": @"image-loaded", @"image_loaded": @YES,
            @"pid": @(NSProcessInfo.processInfo.processIdentifier),
            @"driver_image": image.dli_fname ? [NSString stringWithUTF8String:image.dli_fname] : @"",
            @"application_ready": @NO, @"delegate_ready": @NO, @"menu_ready": @NO,
            @"witness_installed": @NO, @"native_reply": NSNull.null,
            @"alert_seen": @NO, @"button_title": @""} mutableCopy];
        [driver writeState];
        dispatch_async(dispatch_get_main_queue(), ^{ [driver start]; });
    }
}
