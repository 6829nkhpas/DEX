//! Privilege escalation test.
//! Simulates checking that an account cannot assume the roles or modify the resources
//! of another account.

use types::ids::AccountId;

pub struct AuthorizationService;

pub enum ResourceType {
    AccountBalance,
    OrderEntry,
    SystemConfig,
}

#[derive(Debug, PartialEq)]
pub enum AuthError {
    Unauthorized,
}

impl AuthorizationService {
    /// Validates if a given requester (authenticated user) has permission
    /// to perform an action on a target resource owner's behalf.
    pub fn check_permission(
        requester: &AccountId,
        resource_owner: Option<&AccountId>,
        resource_type: ResourceType,
        is_admin: bool,
    ) -> Result<(), AuthError> {
        match resource_type {
            ResourceType::SystemConfig => {
                if !is_admin {
                    return Err(AuthError::Unauthorized);
                }
            }
            _ => {
                // For user-level resources, requester must be the owner, or be an admin
                if let Some(owner) = resource_owner {
                    if requester != owner && !is_admin {
                        return Err(AuthError::Unauthorized);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_privilege_escalation_mitigation() {
        let alice = AccountId::new();
        let bob = AccountId::new();
        let admin = AccountId::new();

        // 1. Alice tries to modify Alice's balance -> OK
        assert_eq!(
            AuthorizationService::check_permission(
                &alice,
                Some(&alice),
                ResourceType::AccountBalance,
                false
            ),
            Ok(())
        );

        // 2. Alice tries to modify Bob's balance -> Error (Privilege Escalation Attempt)
        assert_eq!(
            AuthorizationService::check_permission(
                &alice,
                Some(&bob),
                ResourceType::AccountBalance,
                false
            ),
            Err(AuthError::Unauthorized)
        );

        // 3. Admin modifies Bob's balance -> OK
        assert_eq!(
            AuthorizationService::check_permission(
                &admin,
                Some(&bob),
                ResourceType::AccountBalance,
                true
            ),
            Ok(())
        );

        // 4. Alice tries to modify System Config -> Error
        assert_eq!(
            AuthorizationService::check_permission(&alice, None, ResourceType::SystemConfig, false),
            Err(AuthError::Unauthorized)
        );

        // 5. Admin modifies System Config -> OK
        assert_eq!(
            AuthorizationService::check_permission(&admin, None, ResourceType::SystemConfig, true),
            Ok(())
        );
    }
}
