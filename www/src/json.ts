export interface AccountStatus {
    result_type: string;
    message: any;
}

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
    challenge_type: string;
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

export interface CheckDisplayNameResult {
    type: string;
    value: any;
}

export interface Violation {
    context: Context,
    display_name: string,
}
