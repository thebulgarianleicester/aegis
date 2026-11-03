use aegis_journal::{Journal, Phase};
use proptest::prelude::*;
use std::fs;

#[derive(Debug)]
struct ModelPending {
    id: i64,
    pre: Vec<u8>,
    post: Vec<u8>,
    applied: bool,
}

// Exercise long mixed sequences in an isolated tempdir, including clean reopen.
// This is a functional state-machine test, not a power-failure simulation.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn single_file_transaction_sequences(
        initial in proptest::collection::vec(any::<u8>(), 0..32),
        operations in proptest::collection::vec((0u8..5, proptest::collection::vec(any::<u8>(), 0..32)), 1..40)
    ) {
        let temp=tempfile::tempdir().unwrap();
        let data=temp.path().join("data"); let log=temp.path().join("log");
        fs::create_dir(&data).unwrap(); fs::create_dir(&log).unwrap();
        fs::write(data.join("file"),&initial).unwrap();
        let mut expected=initial;
        let mut pending: Option<ModelPending>=None;
        let mut journal=Journal::open_with_capacity(&data,&log,65536).unwrap();
        for (action, payload) in operations {
            match action {
                0 => {
                    if pending.is_none() {
                        let id=journal.prepare("file",&payload).unwrap();
                        pending=Some(ModelPending{id, pre:expected.clone(),post:payload,applied:false});
                    } else {
                        prop_assert!(journal.prepare("file",&payload).is_err());
                    }
                }
                1 => {
                    match &mut pending {
                        Some(p) if !p.applied => { journal.apply(p.id,&p.post).unwrap(); expected=p.post.clone(); p.applied=true; }
                        Some(p) => prop_assert!(journal.apply(p.id,&p.post).is_err()),
                        None => prop_assert!(journal.apply(0,&payload).is_err()),
                    }
                }
                2 => {
                    match &pending {
                        Some(p) if p.applied => { journal.commit(p.id).unwrap(); pending=None; }
                        Some(p) => prop_assert!(journal.commit(p.id).is_err()),
                        None => prop_assert!(journal.commit(0).is_err()),
                    }
                }
                3 => {
                    if let Some(p)=pending.take() { journal.rollback(p.id).unwrap(); expected=p.pre; }
                    else { prop_assert!(journal.rollback(0).is_err()); }
                }
                _ => { drop(journal); journal=Journal::open_with_capacity(&data,&log,65536).unwrap(); }
            }
            prop_assert_eq!(fs::read(data.join("file")).unwrap(), expected.clone());
            let on_disk=journal.pending().unwrap();
            prop_assert_eq!(on_disk.is_some(),pending.is_some());
            if let (Some(actual),Some(model))=(on_disk,pending.as_ref()) {
                prop_assert_eq!(actual.id,model.id);
                prop_assert_eq!(actual.phase,if model.applied {Phase::Applied} else {Phase::Prepared});
            }
            prop_assert!(journal.object_usage().unwrap() <= 65536);
        }
    }
}
