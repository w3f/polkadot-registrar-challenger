// TODO: Rename
export interface AccountStatus {
    type: string;
    message: any;
}

// TODO: Rename
export interface ValidMessage {
    state: State;
    notifications: Notification[];
}

export interface State {
    context: Context;
    is_fully_verified: boolean;
    completion_timestamp?: any;
    fields: Field[];
}

export interface Context {
    address: string;
    chain: string;
}

export interface Field {
    value: FieldValue;
    challenge: Challenge;
    failed_attempts: number;
}

export interface FieldValue {
    type: string;
    value: string;
}

export interface Challenge {
    type: string;
    content: any;
}

export interface Content {
    expected: Expected;
    second?: Expected;
}

export interface DisplayNameChallenge {
    passed: boolean;
    violations: Violation[];
}

export interface Expected {
    value: string;
    is_verified: boolean;
}

export interface Notification {
    type: string;
    value: any;
}

export interface NotificationFieldContext {
    context: Context;
    field: FieldValue;
}

export interface ManuallyVerified {
    context: Context;
    field: string;
}

export interface CheckDisplayNameResult {
    type: string;
    value: any;
}

export interface Violation {
    context: Context,
    display_name: string,
}
